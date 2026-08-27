# 🏆 Assay — Official Hackathon Submission Package

## Project Overview
- **Project Name**: Assay v10
- **Track**: Evaluator (WASM Author Track) & FACT_CHECK Intent
- **Status**: **`ACTIVE` Champion** on Telegraph Protocol (`REG #1188`)
- **Live WASM Hash (Keccak-256)**: `0x57676cdb4d6d42a32002bc9af057faf98d860672cedc1f58f2244f6e51d9d5b9`
- **GitHub Repository**: [https://github.com/GreatSage-dev/Assay](https://github.com/GreatSage-dev/Assay)
- **Live Browser Demo**: [https://github.com/GreatSage-dev/Assay/blob/main/demo.html](https://github.com/GreatSage-dev/Assay/blob/main/demo.html)

---

## Technical Summary
Assay is a zero-allocation, deterministic WebAssembly scoring module built in `#![no_std]` Rust for the `FACT_CHECK` intent on Telegraph Protocol. 

### Why Assay Beats Simple Overlap Scorers:
Traditional word-overlap algorithms fail on adversarial edge cases. Assay v10 solves this with a **4-Stage Verification Engine**:
1. **30 Antonym Pair Matrix**: Rejects word-swap contradictions (`profit↔loss`, `earned↔lost`, `safe↔compromised`) with instant 0.00 scores.
2. **Negation Modifier & Double-Negation Parity**: Recognizes that double-negations (*"not false"*) cancel out, preserving $0.99+$ scores, while single negations (*"not true"*) trigger instant $0.00$ scores.
3. **Guarded Substring Matching**: Enforces 60% length ratios and blocks negative prefixes (`un-`, `dis-`, `non-`, `mis-`).
4. **Canonical Fact Lock**: Normalizes ordinals (`3rd` = `3`), currency (`$10M` = `10000000`), formatted numbers, and hex addresses.

---

## 📊 Benchmark Progression

| Iteration | Separation Margin | Target | Status |
|-----------|-------------------|--------|--------|
| REG #1083 (v1) | 0.2984 | 0.7916 | Rejected |
| REG #1107 (v5) | 0.6349 | 0.7927 | Rejected |
| REG #1150 (v8) | 0.7793 | 0.7927 | Rejected |
| REG #1164 (v9) | 0.7914 | 0.7927 | Near-miss (-0.0013) |
| **REG #1188 (v10)** | **0.9981** | **0.7927** | **👑 ACTIVE CHAMPION (+0.2054)** |

---

# 📱 Ready-to-Post X (Twitter) Update

Copy and paste the text below to post on X (Twitter):

```text
Shipped Assay v10 for the @Telegraphprotoc hackathon! ⚡

-> Built a #![no_std] zero-allocation Rust WASM scorer for FACT_CHECK
-> Rejected 5 times on edge-case gauntlets
-> Reigned as ACTIVE CHAMPION on attempt 6! 🏆

Assay v10 is now the live active judge for FACT_CHECK on Telegraph Protocol.

How we beat the 0.7927 champion threshold:
1/4: Naive word-overlap is gameable. Swapping "profit" for "loss" or "true" for "not true" tricks basic scorers. Assay v10 implements a 30-antonym matrix + double-negation parity engine.

2/4: Canonical Fact Lock: Extracts and normalizes ordinals (3rd -> 3), currency ($10M -> 10,000,000), formatted numbers, and hex strings. Missing any GT fact = instant 0.00 score.

3/4: Guarded Substring Matching: Blocks false positive matches like "successful" -> "unsuccessful" using 60% ratio checks & negative prefix guards (un-, dis-, non-, mis-).

4/4: Result: 25/25 benchmark fixtures passed (100% ordering accuracy), 0.9981 separation margin (+0.2054 over champion).

Live on Base Sepolia (REG #1188). 
Repo + live in-browser WASM demo: https://github.com/GreatSage-dev/Assay

LFG! 🚀🔥 @Telegraphprotoc
```
