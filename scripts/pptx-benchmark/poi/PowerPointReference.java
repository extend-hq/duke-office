import java.io.ByteArrayInputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import org.apache.poi.sl.extractor.SlideShowExtractor;
import org.apache.poi.sl.usermodel.SlideShow;
import org.apache.poi.sl.usermodel.Slide;
import org.apache.poi.hslf.usermodel.HSLFSlideShow;
import org.apache.poi.xslf.usermodel.XMLSlideShow;

public final class PowerPointReference {
    private static volatile Object consumed;

    private static Map<String, Object> object(Object... pairs) {
        Map<String, Object> result = new LinkedHashMap<>();
        for (int i = 0; i < pairs.length; i += 2) result.put((String) pairs[i], pairs[i + 1]);
        return result;
    }

    private static String json(Object value) {
        if (value == null) return "null";
        if (value instanceof Boolean || value instanceof Number) return value.toString();
        if (value instanceof Map<?, ?> map) {
            List<String> fields = new ArrayList<>();
            for (var entry : map.entrySet()) fields.add(json(entry.getKey()) + ":" + json(entry.getValue()));
            return "{" + String.join(",", fields) + "}";
        }
        if (value instanceof Iterable<?> values) {
            List<String> items = new ArrayList<>();
            for (Object item : values) items.add(json(item));
            return "[" + String.join(",", items) + "]";
        }
        StringBuilder result = new StringBuilder("\"");
        for (char c : value.toString().toCharArray()) {
            switch (c) {
                case '"': result.append("\\\""); break;
                case '\\': result.append("\\\\"); break;
                case '\n': result.append("\\n"); break;
                case '\r': result.append("\\r"); break;
                case '\t': result.append("\\t"); break;
                default:
                    if (c < 32) result.append(String.format("\\u%04x", (int) c));
                    else result.append(c);
            }
        }
        return result.append('"').toString();
    }

    @SuppressWarnings({"rawtypes", "unchecked"})
    private static Map<String, Object> parse(byte[] bytes) throws Exception {
        boolean legacy = bytes.length >= 8 && (bytes[0] & 255) == 0xd0 && (bytes[1] & 255) == 0xcf;
        try (SlideShow show = legacy ? new HSLFSlideShow(new ByteArrayInputStream(bytes)) : new XMLSlideShow(new ByteArrayInputStream(bytes))) {
            SlideShowExtractor extractor = new SlideShowExtractor(show);
            extractor.setSlidesByDefault(true);
            extractor.setNotesByDefault(false);
            extractor.setCommentsByDefault(false);
            extractor.setMasterByDefault(false);
            List<Object> slides = new ArrayList<>();
            for (Object item : show.getSlides()) {
                Slide slide = (Slide) item;
                String body = extractor.getText(slide);
                extractor.setSlidesByDefault(false);
                extractor.setNotesByDefault(true);
                String notes = extractor.getText(slide);
                extractor.setNotesByDefault(false);
                extractor.setSlidesByDefault(true);
                slides.add(object("index", slides.size(), "body_text", body, "notes_text", notes));
            }
            return object("slides", slides, "slide_count", slides.size());
        }
    }

    public static void main(String[] args) {
        try {
            byte[] bytes = Files.readAllBytes(Path.of(args[0]));
            int repeats = Integer.parseInt(args[2]);
            long start = System.nanoTime();
            Map<String, Object> first = parse(bytes);
            double cold = (System.nanoTime() - start) / 1_000_000.0;
            List<Double> samples = new ArrayList<>();
            for (int i = 0; i < repeats; i++) {
                start = System.nanoTime();
                consumed = parse(bytes);
                samples.add((System.nanoTime() - start) / 1_000_000.0);
            }
            if (!args[1].equals("-")) Files.writeString(Path.of(args[1]), json(first));
            System.out.println(json(object("status", "ok", "engine", "apache_poi", "version", "6.0.0", "cold_ms", cold, "samples_ms", samples, "slide_count", first.get("slide_count"))));
        } catch (Throwable error) {
            System.out.println(json(object("status", "error", "engine", "apache_poi", "version", "6.0.0", "error", error.getClass().getName() + ": " + error.getMessage())));
        }
    }
}
