# 🏆 Assay — Active Champion WASM Scorer (`FACT_CHECK`)

[![Telegraph Protocol](https://img.shields.io/badge/Telegraph%20Protocol-Active%20Champion%20%28REG%20%231188%29-brightgreen)](https://integrate.telegraphprotocol.com/dashboard)
[![WASM Target](https://img.shields.io/badge/Target-wasm32--unknown--unknown-blue)](https://webassembly.org)
[![Separation Margin](https://img.shields.io/badge/Separation%20Margin-0.9981-success)](https://github.com/GreatSage-dev/Assay)
[![License](https://img.shields.io/badge/License-MIT-orange)](#)

> **Assay v10** is the reigning **Active Champion WASM Scorer** for the `FACT_CHECK` intent on [Telegraph Protocol](https://telegraphprotocol.com). Built entirely in `#![no_std]` Rust with zero heap allocations, Assay v10 dethroned the incumbent champion by achieving a **`0.9981` separation margin** and **100% ordering accuracy** across the benchmark suite.

---

## ⚡ Live On-Chain Registration (`REG #1188`)

- **Status**: **`ACTIVE` Champion** 👑
- **Intent**: `FACT_CHECK`
- **WASM Raw URL**: `https://raw.githubusercontent.com/GreatSage-dev/Assay/main/assay.wasm`
- **Keccak-256 Hash**: `0x57676cdb4d6d42a32002bc9af057faf98d860672cedc1f58f2244f6e51d9d5b9`
- **Network**: Base Sepolia / Telegraph Protocol Indexer

---

## 📈 Benchmark Progression

| Registration | Version | Separation Margin | Champion Target | Status |
|--------------|---------|-------------------|-----------------|--------|
| `REG #1083` | v1 | `0.2984` | `0.7916` | Rejected |
| `REG #1098` | v3 | `0.4192` | `0.7921` | Rejected |
| `REG #1107` | v5 | `0.6349` | `0.7927` | Rejected |
| `REG #1147` | v7 | `0.7177` | `0.7927` | Rejected |
| `REG #1150` | v8 | `0.7793` | `0.7927` | Rejected |
| `REG #1164` | v9 | `0.7914` | `0.7927` | Rejected (-0.0013) |
| **`REG #1188`** | **v10** | **`0.9981`** | **`0.7927`** | **👑 ACTIVE CHAMPION (+0.2054)** |

---

## 🧠 Architectural Breakthroughs (`no_std` Zero-Allocation Rust)

Assay v10 solves the fundamental vulnerabilities of traditional token-overlap scorers:

### 1. 30 Antonym Pair Matrix
Detects direct word-swap contradictions (`profit↔loss`, `earned↔lost`, `rose↔fell`, `safe↔compromised`, `succeeded↔failed`). If a candidate answer swaps a key GT term for its antonym, the score drops to **`0.0000`**.

### 2. Negation Modifier & Double-Negation Logic
Evaluates adjacent negation modifiers (`not`, `never`, `no`, `without`, `n't`). Correctly recognizes that double-negations (*"not false"*) cancel out, preserving high paraphrase scores ($0.99+$), while single negations (*"not true"*) trigger an instant $0.00$ contradiction score.

### 3. Guarded Substring Matching
Prevents bad actors from gaming substring overlaps. Matches require a 60% minimum length ratio and block words starting with negative prefixes (`un-`, `dis-`, `non-`, `mis-`). `"successful"` will **NEVER** match `"unsuccessful"`.

### 4. Canonical Fact Invariants
Normalizes ordinals (`3rd` = `third` = `3`), currency & scale (`$10M` = `10,000,000` = `10000000`), formatted numbers (`192,841` = `192841`), and hex addresses (`0x...`). Any missing or wrong GT fact results in an **instant `0.0000` score**.

### 5. 22-Pair Semantic Synonym Table
Normalizes domain terms (`tx` = `transaction`, `hacked` = `exploited`, `confirmed` = `verified`), allowing legitimate paraphrases to achieve $0.99 - 1.00$ scores.

### 6. Saturation Scoring Curve v10
- **Good Paraphrases ($\ge 0.35$ recall)**: Mapped to **`[0.99, 1.00]`**.
- **Bad / Off-topic / Yapping ($< 0.35$ recall)**: Quartic crushed ($t^4$) to **`[0.000, 0.001]`**.

---

## 🧪 Comprehensive Test Matrix (25/25 Passed)

```bash
# Execute local WebAssembly test suite
node test.js
```

```
--- ASSAY v10 CHAMPION-SLAYER TEST SUITE ---
[PASS] Exact match                         | Score: 1.0000 | GOOD
[PASS] Paraphrase match                    | Score: 0.9962 | GOOD
[PASS] Ordinal / number-word               | Score: 0.9949 | GOOD
[PASS] Currency / magnitude                | Score: 1.0000 | GOOD
[PASS] Hex address match                   | Score: 1.0000 | GOOD
[PASS] Comma fragmentation                 | Score: 1.0000 | GOOD
[PASS] Double negation                     | Score: 0.9923 | GOOD
[PASS] Verbose good answer                 | Score: 0.9953 | GOOD
[PASS] Synonym: tx/transaction             | Score: 1.0000 | GOOD
[PASS] Synonym: hacked/exploited           | Score: 1.0000 | GOOD
[PASS] Synonym: confirmed/verified         | Score: 1.0000 | GOOD
[PASS] Empty answer                        | Score: 0.0000 | BAD
[PASS] Negation inversion                  | Score: 0.0000 | BAD
[PASS] Wrong block number                  | Score: 0.0000 | BAD
[PASS] Hex address mismatch                | Score: 0.0000 | BAD
[PASS] Antonym: profit/loss                | Score: 0.0000 | BAD
[PASS] Antonym: succeed/fail               | Score: 0.0000 | BAD
[PASS] Missing fact                        | Score: 0.0000 | BAD
[PASS] Hedge stuffing / yapping            | Score: 0.0000 | BAD
[PASS] Antonym: earned/lost                | Score: 0.0000 | BAD
[PASS] Antonym: rose/fell                  | Score: 0.0000 | BAD
[PASS] Antonym: safe/compromised           | Score: 0.0000 | BAD
[PASS] Antonym: confirmed/denied           | Score: 0.0000 | BAD
[PASS] Antonym: active/inactive            | Score: 0.0000 | BAD
[PASS] Prefix inversion (unsuccessful)     | Score: 0.0000 | BAD

Summary: 25/25 tests passed.
Avg GOOD Score: 0.9981 | Avg BAD Score: 0.0000
SEPARATION MARGIN: 0.9981 (Champion Target: 0.7927)
```

---

## 🛠️ Local Build Instructions

```bash
# Add WebAssembly compilation target
rustup target add wasm32-unknown-unknown

# Compile release WASM binary
cargo build --release --target wasm32-unknown-unknown

# Copy binary to root & execute verification test
cp target/wasm32-unknown-unknown/release/assay.wasm assay.wasm
node test.js
```

---

## 📄 License

MIT © [GreatSage-dev](https://github.com/GreatSage-dev)
