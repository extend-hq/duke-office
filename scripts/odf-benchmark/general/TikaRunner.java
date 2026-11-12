import java.io.ByteArrayInputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.List;

import org.apache.tika.metadata.Metadata;
import org.apache.tika.metadata.TikaCoreProperties;
import org.apache.tika.parser.AutoDetectParser;
import org.apache.tika.parser.ParseContext;
import org.apache.tika.sax.BodyContentHandler;

/** Warm in-process Apache Tika timings, in the same record shape as the Rust runners. */
public class TikaRunner {
    static String json(String value) {
        StringBuilder out = new StringBuilder("\"");
        for (char ch : value.toCharArray()) {
            switch (ch) {
                case '"': out.append("\\\""); break;
                case '\\': out.append("\\\\"); break;
                case '\n': out.append("\\n"); break;
                case '\r': out.append("\\r"); break;
                case '\t': out.append("\\t"); break;
                default: if (ch < 0x20) out.append(String.format("\\u%04x", (int) ch)); else out.append(ch);
            }
        }
        return out.append('"').toString();
    }

    static String parse(AutoDetectParser parser, byte[] bytes, String name) throws Exception {
        BodyContentHandler handler = new BodyContentHandler(-1);
        Metadata metadata = new Metadata();
        metadata.set(TikaCoreProperties.RESOURCE_NAME_KEY, name);
        parser.parse(new ByteArrayInputStream(bytes), handler, metadata, new ParseContext());
        return handler.toString();
    }

    public static void main(String[] args) throws Exception {
        List<String> paths = Files.readAllLines(Paths.get(args[0]));
        Path output = Paths.get(args[1]);
        AutoDetectParser parser = new AutoDetectParser();
        for (String line : paths) {
            if (line.isEmpty()) continue;
            Path file = Paths.get(line);
            String family = file.getParent().getFileName().toString();
            String name = file.getFileName().toString();
            String key = family + "/" + name;
            byte[] bytes = Files.readAllBytes(file);
            String text = null;
            String error = "";
            long firstStart = System.nanoTime();
            try {
                text = parse(parser, bytes, name);
            } catch (Throwable problem) {
                error = String.valueOf(problem);
            }
            double first = (System.nanoTime() - firstStart) / 1e6;
            double total = 0;
            double min = Double.MAX_VALUE;
            int reps = 0;
            if (text != null && first < 5000) {
                long begin = System.nanoTime();
                while (reps < 3 || ((System.nanoTime() - begin) < 250_000_000L && reps < 200)) {
                    long start = System.nanoTime();
                    try { parse(parser, bytes, name); } catch (Throwable ignored) { }
                    double elapsed = (System.nanoTime() - start) / 1e6;
                    total += elapsed;
                    min = Math.min(min, elapsed);
                    reps++;
                }
            }
            if (reps == 0) { total = first; min = first; reps = 1; }
            if (text != null) {
                Path directory = output.resolve(family);
                Files.createDirectories(directory);
                Files.write(directory.resolve(name + ".txt"), text.getBytes(StandardCharsets.UTF_8));
            }
            System.out.println("{\"file\":" + json(key) + ",\"ok\":" + (text != null) + ",\"error\":" + json(error)
                + ",\"bytes\":" + bytes.length + ",\"chars\":" + (text == null ? 0 : text.length())
                + String.format(",\"first_ms\":%.4f,\"mean_ms\":%.4f,\"min_ms\":%.4f,\"reps\":%d}", first, total / reps, min, reps));
            System.out.flush();
        }
    }
}
