import { createHash } from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";
import { execFile } from "node:child_process";

const execFileAsync = promisify(execFile);
const ROOT = process.cwd();
const MANIFEST_PATH = "data/crossword/manifest.json";
const MODEL_BASE_URL = "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main";

const modelFiles = [
  "config.json",
  "tokenizer.json",
  "tokenizer_config.json",
  "special_tokens_map.json",
  "vocab.txt",
  "README.md",
  "onnx/model_quantized.onnx",
];

function absolute(relativePath) {
  return path.join(ROOT, relativePath);
}

async function exists(filePath) {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}

async function sha256(filePath) {
  const hash = createHash("sha256");
  hash.update(await fs.readFile(filePath));
  return hash.digest("hex");
}

async function download(url, destination) {
  await fs.mkdir(path.dirname(destination), { recursive: true });
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Failed to download ${url}: ${response.status} ${response.statusText}`);
  }
  const buffer = Buffer.from(await response.arrayBuffer());
  await fs.writeFile(destination, buffer);
}

async function ensureDownloaded(url, destination, expectedSha256) {
  if ((await exists(destination)) && (!expectedSha256 || (await sha256(destination)) === expectedSha256)) {
    return;
  }

  console.log(`Downloading ${url}`);
  await download(url, destination);

  if (expectedSha256) {
    const actualSha256 = await sha256(destination);
    if (actualSha256 !== expectedSha256) {
      throw new Error(`Checksum mismatch for ${destination}: expected ${expectedSha256}, got ${actualSha256}`);
    }
  }
}

async function ensureWordNet(manifest) {
  const wordnet = manifest.assets.wordnet;
  const runtimePath = absolute(wordnet.runtimePath);
  const requiredFiles = ["data.noun", "data.verb", "data.adj", "data.adv", "cntlist"];
  const isReady = (await Promise.all(requiredFiles.map((file) => exists(path.join(runtimePath, file))))).every(Boolean);
  if (isReady) {
    console.log("WordNet dictionary assets already present.");
    return;
  }

  const archivePath = absolute(wordnet.archivePath);
  await ensureDownloaded(wordnet.sourceUrl, archivePath, wordnet.archiveSha256);

  const extractRoot = path.dirname(runtimePath);
  await fs.rm(extractRoot, { recursive: true, force: true });
  await fs.mkdir(extractRoot, { recursive: true });
  await execFileAsync("tar", ["-xzf", archivePath, "-C", extractRoot]);
  console.log(`Prepared WordNet dictionary assets at ${wordnet.runtimePath}.`);
}

async function ensureModel(manifest) {
  const model = manifest.assets.embeddingModel;
  const runtimePath = absolute(model.runtimePath);
  const checksums = new Map([
    ["tokenizer.json", model.checksums.tokenizerJsonSha256],
    ["vocab.txt", model.checksums.vocabTxtSha256],
    ["onnx/model_quantized.onnx", model.checksums.quantizedOnnxSha256],
  ]);

  for (const file of modelFiles) {
    const destination = path.join(runtimePath, file);
    await ensureDownloaded(`${MODEL_BASE_URL}/${file}`, destination, checksums.get(file));
  }

  console.log(`Prepared embedding model assets at ${model.runtimePath}.`);
}

async function main() {
  const manifest = JSON.parse(await fs.readFile(absolute(MANIFEST_PATH), "utf8"));
  await ensureWordNet(manifest);
  await ensureModel(manifest);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
