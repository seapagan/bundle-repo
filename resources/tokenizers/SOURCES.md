# Embedded tokenizer sources

BundleRepo embeds the tokenizer JSON files below with `rust-embed`
compression. It retrieves and parses only the selected model family's asset
at runtime and never downloads tokenizer data while running.

All downloads were made on 2026-08-03 with redirects, HTTP failure detection,
and retries enabled. Each file was downloaded to a temporary location, checked
as full JSON rather than HTML or a pointer, parsed as JSON, and loaded with the
locked Rust `tokenizers` crate before being installed here.

## `deepseek-v3.json`

- Local path: `resources/tokenizers/deepseek-v3.json`
- Model choice: `deepseek-v3`
- Official repository:
  <https://huggingface.co/deepseek-ai/DeepSeek-V3>
- Immutable revision: `e815299b0bcbac849fa540c768ef21845365c9eb`
- Revision-pinned download:
  <https://huggingface.co/deepseek-ai/DeepSeek-V3/resolve/e815299b0bcbac849fa540c768ef21845365c9eb/tokenizer.json?download=true>
- Download date: 2026-08-03
- Byte size: `7,847,652`
- SHA-256:
  `621ac2e32d0dba658404412318818aaa8ce8cda492e59830109d8da6b517fb41`
- Distribution includes both:
  - the DeepSeek MIT notice at
    `resources/tokenizers/licenses/DeepSeek-MIT.txt`; and
  - the pinned DeepSeek V3 model licence at
    `resources/tokenizers/licenses/DeepSeek-V3-Model-License.txt`.
- Licence sources: pinned official DeepSeek V3 `LICENSE-CODE` and
  `LICENSE-MODEL`

## `deepseek-r1.json`

- Local path: `resources/tokenizers/deepseek-r1.json`
- Model choices: `deepseek-r1` and the legacy `deepseek` alias, for the full
  R1-0528 release rather than a Qwen- or Llama-based distill
- Official repository:
  <https://huggingface.co/deepseek-ai/DeepSeek-R1-0528>
- Immutable revision: `4236a6af538feda4548eca9ab308586007567f52`
- Revision-pinned download:
  <https://huggingface.co/deepseek-ai/DeepSeek-R1-0528/resolve/4236a6af538feda4548eca9ab308586007567f52/tokenizer.json?download=true>
- Download date: 2026-08-03
- Byte size: `7,847,602`
- SHA-256:
  `ecb6f9fc369894346f0511f4074ca75cee5cd5f3b06d02f1ba35fcd39f8e121d`
- Upstream licence: MIT
- Bundled notice: [`licenses/DeepSeek-MIT.txt`](licenses/DeepSeek-MIT.txt)

### V3 and R1 comparison

The official files are not byte-identical and are not tokenization-equivalent.
Their model vocabulary, merges, normalizer, pre-tokenizer, post-processor, and
decoder are identical. Their added-token definitions differ at IDs `128798`
and `128799`: V3 assigns placeholder tokens, while full R1-0528 assigns
`<think>` and `</think>`. They therefore remain separate embedded assets.

## `deepseek-v4.json`

- Local path: `resources/tokenizers/deepseek-v4.json`
- Model choice: `deepseek-v4`
- Official repository:
  <https://huggingface.co/deepseek-ai/DeepSeek-V4-Pro>
- Immutable revision: `b5968e9190ef611bbf34a7229255be88a0e937c1`
- Revision-pinned download:
  <https://huggingface.co/deepseek-ai/DeepSeek-V4-Pro/resolve/b5968e9190ef611bbf34a7229255be88a0e937c1/tokenizer.json?download=true>
- Download date: 2026-08-03
- Byte size: `6,367,146`
- SHA-256:
  `8f9f37ca37fdc4f5fd36d5cf4d3b0e8392edb4e894fd10cc0d70b4957c8633cf`
- Upstream licence: MIT
- Bundled notice: [`licenses/DeepSeek-MIT.txt`](licenses/DeepSeek-MIT.txt)
- Notice verification: the pinned V4 repository's `LICENSE` has the same MIT
  text and `Copyright (c) 2023 DeepSeek` notice as V3 and R1.

## `glm-5.2.json`

- Local path: `resources/tokenizers/glm-5.2.json`
- Model choice: `glm5.2`
- Official repository: <https://huggingface.co/zai-org/GLM-5.2>
- Immutable revision: `08c85fae01c239464d7038182e8dd8c5b1cba78f`
- Revision-pinned download:
  <https://huggingface.co/zai-org/GLM-5.2/resolve/08c85fae01c239464d7038182e8dd8c5b1cba78f/tokenizer.json?download=true>
- Download date: 2026-08-03
- Byte size: `20,217,442`
- SHA-256:
  `19e773648cb4e65de8660ea6365e10acca112d42a854923df93db4a6f333a82d`
- Upstream licence: MIT
- Bundled notice: [`licenses/GLM-5.2-MIT.txt`](licenses/GLM-5.2-MIT.txt)
- Licence provenance: the tokenizer revision predates the repository's
  `LICENSE` file. The exact notice was retrieved from the current official
  revision `b4734de4facf877f85769a911abafc5283eab3d9`, where it records
  `Copyright (c) 2026 Zhipu AI`.
