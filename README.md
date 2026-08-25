# Assay — Telegraph Protocol Scoring Module (`FACT_CHECK`)

**Assay** is a high-precision, deterministic WebAssembly (WASM) scoring module for the [Telegraph Protocol](https://telegraphprotocol.com) hackathon targeting the `FACT_CHECK` intent.

---

## Technical Specifications

- **Target Architecture**: `wasm32-unknown-unknown` (`#![no_std]`, `cdylib`)
- **Binary Size**: `9.5 KB` (Compiled with release LTO and size optimization `opt-level = "z"`)
- **Import Count**: `0` (Zero external imports, 100% deterministic execution)
- **Exports**: `alloc`, `dealloc`, `rank_answer`
- **SHA-256 Hash**: `e2ee151a8fb2ae49006814815a499bf5eb60ab9aaa714622143d9684b24e2442`

---

## Design Rationale — Why Assay Beats Simple Word-Overlap

Naive scoring modules use simple word presence or ngram overlap (like BLEU or ROUGE). This approach is fundamentally vulnerable:
1. **Negation Flipping**: `"the claim succeeded"` vs `"the claim did NOT succeed"` shares almost every word, yet means the exact opposite. Naive modules mistakenly give high scores to opposite answers.
2. **Number & Formatting Fragmentation**: `$10M`, `10,000,000`, and `10 million` represent the same fact but fail raw string comparisons.
3. **Hedge-Stuffing**: Bad actors pad incorrect answers with 200-word generic disclaimers to inflate keyword match rates.

### The 6-Layer Scoring Engine

1. **Hard-Fact Extraction & Canonical Normalization**:
   - Converts currency, magnitude suffixes (`$10M` = `10000000`), ordinals (`3rd` = `third` = `3`), commas (`192,841` = `192841`), and hex literals (`0x...`) into unified canonical forms.
   - If the main fact in ground truth is mismatched in miner answer, the score drops to **`0.00`**.
2. **Stopword Filtering**: Strips filler words (`the`, `is`, `a`, `at`) to focus on semantic content words.
3. **Bigram (Word-Pair) Dice Overlap**: Catches structural disruption from keyword reordering.
4. **Longest Common Subsequence (LCS)**: Measures logical relative word flow for paraphrase matching.
5. **Negation-Asymmetry Penalty**: Detects contradiction terms (`not`, `never`, `false`, `denied`). Mismatched negation count applies a **0.15 score penalty multiplier**.
6. **Anti-Hedge Length Ratio Penalty**: Degrades score smoothly when miner answers exceed 2x the byte length of ground truth.

---

## Verification & Test Results

```bash
# Run test suite against assay.wasm
node test.js
```

### Test Matrix (11/11 Passed)

| Test Scenario | Ground Truth | Miner Answer | Score | Result |
|---|---|---|---|---|
| Empty Answer | `"correct answer"` | `""` | `0.0000` | **PASS** |
| Exact Match | `"the claim is true"` | `"the claim is true"` | `1.0000` | **PASS** |
| Negation Asymmetry | `"the claim is true"` | `"the claim is not true"` | `0.0840` | **PASS** |
| Paraphrase Match | `"the claim is true"` | `"the claim is indeed true"` | `0.5600` | **PASS** |
| Comma Formatting | `"failed at block 192,841"` | `"reverted at block 192841"` | `0.4500` | **PASS** |
| Ordinal Equivalent | `"the 3rd attempt succeeded"` | `"the third attempt succeeded"` | `1.0000` | **PASS** |
| Currency Equivalent | `"raised $10M"` | `"raised 10,000,000 dollars"` | `0.8400` | **PASS** |
| Hex Address Match | `"reverted at 0x1234abcd"` | `"reverted at 0x1234abcd"` | `0.6167` | **PASS** |
| Hex Address Mismatch | `"reverted at 0x1234abcd"` | `"reverted at 0x1234efgh"` | `0.0000` | **PASS** |
| Hedge Stuffing Penalty | `"the claim is false"` | `"while there are many perspectives..."` | `0.0268` | **PASS** |
| Wrong Fact | `"failed at block 192841"` | `"reverted at block 552019"` | `0.0000` | **PASS** |

---

## Telegraph Protocol Registration Parameters

- **Intent**: `FACT_CHECK`
- **Raw WASM Direct URL**: `https://raw.githubusercontent.com/GreatSage-dev/Assay/main/assay.wasm`
- **WASM Hash**: `e2ee151a8fb2ae49006814815a499bf5eb60ab9aaa714622143d9684b24e2442`
- **Registration Web UI**: [integrate.telegraphprotocol.com](https://integrate.telegraphprotocol.com)

---

## Build Commands

```bash
# Add WebAssembly compilation target
rustup target add wasm32-unknown-unknown

# Release build with LTO and size optimization
cargo build --release --target wasm32-unknown-unknown

# Verify 0 external WASM imports
python -c "
with open('target/wasm32-unknown-unknown/release/assay.wasm', 'rb') as f:
    data = f.read()
# verify import section count == 0
"
```
