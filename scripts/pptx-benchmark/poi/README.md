# Apache POI reference

Apache POI 6.0.0 reads legacy PPT through HSLF and OOXML through XMLSlideShow. The adapter extracts slide body text and speaker notes, excluding comments and master text. It does not render slides or establish independent visual ground truth.

Use Java 17 or newer and Python 3.12 or newer:

```sh
python3 scripts/pptx-benchmark/poi_setup.py
python3 scripts/pptx-benchmark/poi_reference.py \
  --manifest output/pptx-benchmark-20261106/poi-runtime/legacy-tuning-cohort.json \
  --output output/pptx-benchmark-20261106/poi-legacy-tuning
python3 scripts/pptx-benchmark/legacy_poi_comparison.py
```

The setup downloads Maven 3.9.11 from Maven Central, verifies its published SHA-512, uses an isolated dependency repository, and records the compiler/runtime, source, class and dependency identities. An existing output directory with measurements is preserved; use a new path for another run. `JAVA_HOME` selects the build runtime; `--java` selects a reference-run executable.

The initial 328-input cohort includes the original legacy files, expanded OLE tuning files, and 111 confirmed legacy additions. Reserved validation inputs are excluded. Forty files fail POI parsing. Comparisons on the 288 accepted inputs include native-parser failures as empty output, align slides by order, normalize NFKC text, remove whitespace, and measure character five-gram overlap separately for body text and notes. Agreement with POI is not proof of correctness; inspect disagreements against the binary specification or an independent application before changing the parser.

Zero-repeat runs measure acceptance and return content. Their diagnostic cold times exclude JVM startup and file reading, and concurrent diagnostic runs do not support latency rankings. A fair speed comparison requires a dedicated uncontended run and explicit JVM warmup methodology.
