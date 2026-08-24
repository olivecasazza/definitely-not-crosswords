# Crossword Data Assets

Local generation assets for topic-scored crossword construction.

These files are intentionally ignored by Git because they are large binary/static data assets.
They are expected to exist locally at these paths:

- `data/crossword/wordnet/dict/` - Princeton WordNet 3.1 dictionary files.
- `data/crossword/models/all-MiniLM-L6-v2/` - Xenova ONNX packaging of `all-MiniLM-L6-v2`.
- `data/crossword/downloads/wn3.1.dict.tar.gz` - original WordNet archive.

The manifest in `manifest.json` records source URLs, key files, sizes, and checksums.

