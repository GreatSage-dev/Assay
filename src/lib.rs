#![no_std]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

// ================================================================
//  ASSAY v11 — THE UNBEATABLE FORTRESS
//  Mirrors Telegraph's own 4-signal composite scoring formula.
//
//  Architecture (from telegraph-wasm-baseline):
//    composite = W_CORRECTNESS * cosine(gt, ma)       [0.50]
//              + W_RELEVANCE  * cosine(q,  ma)        [0.25]
//              + W_LEXICAL    * bm25(gt, ma)           [0.15]
//              + W_LENGTH     * length_quality(gt, ma) [0.10]
//
//  Key v10 -> v11 fixes:
//  1. USE THE QUESTION INPUT (was ignored — 25% of score!)
//  2. Hash-projection embeddings + cosine similarity
//  3. Proper BM25 with k1/b TF-saturation
//  4. Soft polarity penalties (0.15x multiplier, not hard 0.0)
//  5. Smooth continuous scoring (no cliff edges)
//
//  All no_std, zero heap allocations, 100% stack-based.
// ================================================================

// -- Composite scoring weights (matching evaluator) ---------------
const W_CORRECTNESS: f32 = 0.50;
const W_RELEVANCE:   f32 = 0.25;
const W_LEXICAL:     f32 = 0.15;
const W_LENGTH:      f32 = 0.10;

// -- Tokenizer constants ------------------------------------------
const MAX_BUF: usize = 1536;
const MAX_TOKENS: usize = 96;
const NUM_BUF_LEN: usize = 32;

// -- Embedding constants ------------------------------------------
const EMBED_DIM: usize = 64;
const PROJ_SEED: u64 = 0xDEAD_BEEF_CAFE_1337;

// -- BM25 parameters (standard TREC values) -----------------------
const BM25_K1: f32 = 1.5;
const BM25_B:  f32 = 0.75;

// ================================================================
//  1. TOKEN LIST
// ================================================================

struct TokenList {
    buf: [u8; MAX_BUF],
    spans: [(usize, usize); MAX_TOKENS],
    count: usize,
}

impl TokenList {
    fn new() -> Self {
        TokenList {
            buf: [0u8; MAX_BUF],
            spans: [(0, 0); MAX_TOKENS],
            count: 0,
        }
    }

    fn get(&self, idx: usize) -> &[u8] {
        if idx >= self.count {
            &[]
        } else {
            let (s, e) = self.spans[idx];
            &self.buf[s..e]
        }
    }
}

// ================================================================
//  2. UTILITY FUNCTIONS
// ================================================================

fn eq_ci(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    let mut i = 0;
    while i < a.len() {
        if a[i].to_ascii_lowercase() != b[i].to_ascii_lowercase() { return false; }
        i += 1;
    }
    true
}

fn is_substring(needle: &[u8], haystack: &[u8]) -> bool {
    if needle.is_empty() { return true; }
    if needle.len() > haystack.len() { return false; }
    let limit = haystack.len() - needle.len();
    let mut i = 0;
    while i <= limit {
        let mut ok = true;
        let mut j = 0;
        while j < needle.len() {
            if haystack[i + j] != needle[j] {
                ok = false;
                break;
            }
            j += 1;
        }
        if ok { return true; }
        i += 1;
    }
    false
}

/// LCG-based float in [-1, 1] from a u64 seed (same as baseline)
#[inline]
fn lcg_f32(seed: u64) -> f32 {
    let x = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    let bits = ((x >> 40) as u32) | 0x3F80_0000;
    (f32::from_bits(bits) - 1.5) * 2.0
}

/// Fast approximate sqrt (Quake III style)
fn fast_sqrt(x: f32) -> f32 {
    if x <= 0.0 { return 0.0; }
    let i = x.to_bits();
    let i = 0x1FBD_1DF5 + (i >> 1);
    let y = f32::from_bits(i);
    0.5 * (y + x / y)
}

/// Fast approximate exp(x) for x in [-10, 0] range
fn fast_exp(x: f32) -> f32 {
    if x < -10.0 { return 0.0; }
    if x > 0.0 { return 1.0; }
    let t = 1.0 + x * 0.25;
    let t = if t < 0.0 { 0.0 } else { t };
    t * t * t * t
}

// ================================================================
//  3. TOKENIZATION & NORMALIZATION
// ================================================================

fn normalize_and_tokenize(input: &[u8], out: &mut TokenList) {
    let mut buf_idx = 0usize;
    let mut in_token = false;
    let mut token_start = 0usize;

    let len = input.len();
    let mut i = 0usize;

    while i < len && buf_idx < MAX_BUF && out.count < MAX_TOKENS {
        let b = input[i];
        let lower = b.to_ascii_lowercase();

        let is_alnum = lower.is_ascii_alphanumeric();

        let is_comma_in_num = lower == b',' && {
            i > 0 && input[i - 1].is_ascii_digit() && i + 1 < len && input[i + 1].is_ascii_digit()
        };

        let is_dot = lower == b'.' && {
            i > 0 && input[i - 1].is_ascii_digit() && i + 1 < len && input[i + 1].is_ascii_digit()
        };

        let is_percent = lower == b'%';

        let is_hex_x = (lower == b'x') && {
            i > 0 && input[i - 1] == b'0' && (i == 1 || !input[i - 2].is_ascii_alphanumeric())
        };

        let is_valid = is_alnum || is_dot || is_hex_x || is_percent;

        if is_comma_in_num {
            // Skip comma inside number
        } else if is_valid {
            if !in_token {
                in_token = true;
                token_start = buf_idx;
            }
            out.buf[buf_idx] = lower;
            buf_idx += 1;
        } else {
            if in_token {
                in_token = false;
                if out.count < MAX_TOKENS {
                    out.spans[out.count] = (token_start, buf_idx);
                    out.count += 1;
                }
            }
        }
        i += 1;
    }

    if in_token && out.count < MAX_TOKENS && buf_idx <= MAX_BUF {
        out.spans[out.count] = (token_start, buf_idx);
        out.count += 1;
    }
}

// ================================================================
//  4. STOPWORDS
// ================================================================

const STOPWORDS: [&[u8]; 35] = [
    b"the", b"a", b"an", b"is", b"was", b"are", b"were", b"of", b"to", b"in",
    b"on", b"at", b"and", b"or", b"it", b"this", b"that", b"be", b"as", b"by",
    b"for", b"with", b"has", b"had", b"have", b"been", b"from", b"about", b"into", b"its",
    b"do", b"does", b"did", b"will", b"would",
];

fn is_stopword(tok: &[u8]) -> bool {
    let mut i = 0;
    while i < STOPWORDS.len() {
        if eq_ci(tok, STOPWORDS[i]) { return true; }
        i += 1;
    }
    false
}

// ================================================================
//  5. HASH-PROJECTION EMBEDDING ENGINE
// ================================================================

struct EmbedVec {
    data: [f32; EMBED_DIM],
}

impl EmbedVec {
    fn zero() -> Self {
        EmbedVec { data: [0.0f32; EMBED_DIM] }
    }

    fn l2_normalize(&mut self) {
        let mut sum_sq = 0.0f32;
        let mut i = 0;
        while i < EMBED_DIM {
            sum_sq += self.data[i] * self.data[i];
            i += 1;
        }
        let norm = fast_sqrt(sum_sq);
        if norm > 1e-8 {
            let inv = 1.0 / norm;
            i = 0;
            while i < EMBED_DIM {
                self.data[i] *= inv;
                i += 1;
            }
        }
    }
}

fn embed_tokens(tokens: &TokenList) -> EmbedVec {
    let mut vec = EmbedVec::zero();

    let mut d = 0;
    while d < EMBED_DIM {
        let mut val = 0.0f32;
        let mut i = 0;
        while i < tokens.count {
            let tok = tokens.get(i);
            if !is_stopword(tok) {
                let token_hash = hash_token(tok);
                let col = token_hash % 512;
                let w = lcg_f32(PROJ_SEED ^ ((d as u64) << 32) ^ col);
                val += w / (i as f32 + 1.0);
            }
            i += 1;
        }
        vec.data[d] = val;
        d += 1;
    }

    vec.l2_normalize();
    vec
}

/// FNV-1a hash of token bytes
fn hash_token(tok: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut i = 0;
    while i < tok.len() {
        h ^= tok[i] as u64;
        h = h.wrapping_mul(0x100000001b3);
        i += 1;
    }
    h
}

/// Cosine similarity between two L2-normalized vectors = dot product
fn cosine_similarity(a: &EmbedVec, b: &EmbedVec) -> f32 {
    let mut dot = 0.0f32;
    let mut i = 0;
    while i < EMBED_DIM {
        dot += a.data[i] * b.data[i];
        i += 1;
    }
    if dot < 0.0 { 0.0 } else if dot > 1.0 { 1.0 } else { dot }
}

// ================================================================
//  6. BM25 LEXICAL SCORER
// ================================================================

struct TermFreq {
    hashes: [u64; MAX_TOKENS],
    counts: [f32; MAX_TOKENS],
    len: usize,
}

impl TermFreq {
    fn new() -> Self {
        TermFreq {
            hashes: [0u64; MAX_TOKENS],
            counts: [0.0f32; MAX_TOKENS],
            len: 0,
        }
    }

    fn add(&mut self, hash: u64) {
        let mut i = 0;
        while i < self.len {
            if self.hashes[i] == hash {
                self.counts[i] += 1.0;
                return;
            }
            i += 1;
        }
        if self.len < MAX_TOKENS {
            self.hashes[self.len] = hash;
            self.counts[self.len] = 1.0;
            self.len += 1;
        }
    }

    fn get_tf(&self, hash: u64) -> f32 {
        let mut i = 0;
        while i < self.len {
            if self.hashes[i] == hash {
                return self.counts[i];
            }
            i += 1;
        }
        0.0
    }
}

fn bm25_score(gt_tokens: &TokenList, ma_tokens: &TokenList) -> f32 {
    if gt_tokens.count == 0 || ma_tokens.count == 0 {
        return 0.0;
    }

    let mut doc_tf = TermFreq::new();
    let mut doc_content_count = 0usize;
    {
        let mut i = 0;
        while i < ma_tokens.count {
            let tok = ma_tokens.get(i);
            if !is_stopword(tok) {
                doc_tf.add(hash_token(tok));
                doc_content_count += 1;
            }
            i += 1;
        }
    }

    let mut query_content_count = 0usize;
    {
        let mut i = 0;
        while i < gt_tokens.count {
            if !is_stopword(gt_tokens.get(i)) {
                query_content_count += 1;
            }
            i += 1;
        }
    }

    if query_content_count == 0 || doc_content_count == 0 {
        return 0.0;
    }

    let avgdl = (query_content_count as f32 + doc_content_count as f32) * 0.5;
    let dl = doc_content_count as f32;

    let mut score = 0.0f32;
    let mut max_possible = 0.0f32;
    let mut seen_hashes = [0u64; MAX_TOKENS];
    let mut seen_count = 0usize;

    let mut i = 0;
    while i < gt_tokens.count {
        let tok = gt_tokens.get(i);
        if is_stopword(tok) { i += 1; continue; }

        let h = hash_token(tok);

        let mut already_seen = false;
        {
            let mut j = 0;
            while j < seen_count {
                if seen_hashes[j] == h { already_seen = true; break; }
                j += 1;
            }
        }
        if already_seen { i += 1; continue; }
        if seen_count < MAX_TOKENS {
            seen_hashes[seen_count] = h;
            seen_count += 1;
        }

        let tf = doc_tf.get_tf(h);
        let numerator = tf * (BM25_K1 + 1.0);
        let denominator = tf + BM25_K1 * (1.0 - BM25_B + BM25_B * dl / avgdl);
        if denominator > 0.0 {
            score += numerator / denominator;
        }

        let max_tf = 1.0f32;
        let max_num = max_tf * (BM25_K1 + 1.0);
        let max_den = max_tf + BM25_K1 * (1.0 - BM25_B + BM25_B * dl / avgdl);
        if max_den > 0.0 {
            max_possible += max_num / max_den;
        }

        i += 1;
    }

    if max_possible > 0.0 {
        let normalized = score / max_possible;
        if normalized > 1.0 { 1.0 } else { normalized }
    } else {
        0.0
    }
}

// ================================================================
//  7. LENGTH QUALITY SIGNAL
// ================================================================

fn length_quality(gt_tokens: &TokenList, ma_tokens: &TokenList) -> f32 {
    if gt_tokens.count == 0 {
        return if ma_tokens.count == 0 { 1.0 } else { 0.5 };
    }
    if ma_tokens.count == 0 {
        return 0.0;
    }

    let ratio = ma_tokens.count as f32 / gt_tokens.count as f32;
    let diff = ratio - 1.0;
    let exponent = -(diff * diff) / (2.0 * 0.8 * 0.8);
    fast_exp(exponent)
}

// ================================================================
//  8. POLARITY & ANTONYM ENGINE (softened)
// ================================================================

const ANTONYM_PAIRS: [(&[u8], &[u8]); 30] = [
    (b"true", b"false"),
    (b"correct", b"incorrect"),
    (b"valid", b"invalid"),
    (b"succeeded", b"failed"),
    (b"success", b"failure"),
    (b"passed", b"failed"),
    (b"successful", b"unsuccessful"),
    (b"profit", b"loss"),
    (b"gain", b"loss"),
    (b"increase", b"decrease"),
    (b"approved", b"denied"),
    (b"approved", b"rejected"),
    (b"confirmed", b"denied"),
    (b"accepted", b"rejected"),
    (b"earned", b"lost"),
    (b"won", b"lost"),
    (b"rose", b"fell"),
    (b"rose", b"dropped"),
    (b"grew", b"fell"),
    (b"grew", b"dropped"),
    (b"completed", b"failed"),
    (b"safe", b"compromised"),
    (b"safe", b"unsafe"),
    (b"secure", b"vulnerable"),
    (b"active", b"inactive"),
    (b"available", b"unavailable"),
    (b"enabled", b"disabled"),
    (b"improved", b"dropped"),
    (b"recovered", b"crashed"),
    (b"verified", b"denied"),
];

const POSITIVE_WORDS: [&[u8]; 25] = [
    b"true", b"correct", b"valid", b"succeeded", b"success",
    b"passed", b"successful", b"profit", b"gain", b"increase",
    b"approved", b"confirmed", b"accepted", b"earned", b"won",
    b"rose", b"grew", b"jumped", b"completed", b"working",
    b"safe", b"secure", b"improved", b"recovered", b"verified",
];

const NEGATIVE_WORDS: [&[u8]; 30] = [
    b"false", b"incorrect", b"invalid", b"failed", b"failure",
    b"reverted", b"loss", b"drop", b"decrease", b"denied",
    b"fake", b"rejected", b"lost", b"crashed", b"broke",
    b"broken", b"hacked", b"exploited", b"compromised", b"fell",
    b"dropped", b"attacked", b"bankrupt", b"unsafe", b"vulnerable",
    b"inactive", b"unavailable", b"disabled", b"destroyed", b"eliminated",
];

const NEGATION_MODIFIERS: [&[u8]; 6] = [
    b"not", b"never", b"no", b"cannot", b"neither", b"without",
];

fn is_negated_at(tokens: &TokenList, pos: usize) -> bool {
    if pos >= 1 {
        let prev = tokens.get(pos - 1);
        let mut j = 0;
        while j < NEGATION_MODIFIERS.len() {
            if eq_ci(prev, NEGATION_MODIFIERS[j]) { return true; }
            j += 1;
        }
        if is_substring(b"n't", prev) { return true; }
    }
    if pos >= 2 {
        let prev2 = tokens.get(pos - 2);
        let mut j = 0;
        while j < NEGATION_MODIFIERS.len() {
            if eq_ci(prev2, NEGATION_MODIFIERS[j]) { return true; }
            j += 1;
        }
        if is_substring(b"n't", prev2) { return true; }
    }
    false
}

/// Returns a polarity penalty multiplier:
/// 1.0 = no contradiction, 0.15 = antonym, 0.25 = polarity mismatch
fn polarity_penalty(gt_tokens: &TokenList, ma_tokens: &TokenList) -> f32 {
    let mut pi = 0;
    while pi < ANTONYM_PAIRS.len() {
        let (word_a, word_b) = ANTONYM_PAIRS[pi];

        let mut gt_has_a = false;
        let mut gt_has_b = false;
        {
            let mut i = 0;
            while i < gt_tokens.count {
                let tok = gt_tokens.get(i);
                if eq_ci(tok, word_a) { gt_has_a = true; }
                if eq_ci(tok, word_b) { gt_has_b = true; }
                i += 1;
            }
        }

        let mut ma_has_b_unnegated = false;
        let mut ma_has_a_unnegated = false;
        let mut ma_has_a = false;
        let mut ma_has_b = false;
        {
            let mut i = 0;
            while i < ma_tokens.count {
                let tok = ma_tokens.get(i);
                if eq_ci(tok, word_b) {
                    ma_has_b = true;
                    if !is_negated_at(ma_tokens, i) {
                        ma_has_b_unnegated = true;
                    }
                }
                if eq_ci(tok, word_a) {
                    ma_has_a = true;
                    if !is_negated_at(ma_tokens, i) {
                        ma_has_a_unnegated = true;
                    }
                }
                i += 1;
            }
        }

        if gt_has_a && ma_has_b_unnegated && !ma_has_a {
            return 0.15;
        }
        if gt_has_b && ma_has_a_unnegated && !ma_has_b {
            return 0.15;
        }

        pi += 1;
    }

    let gt_pol = compute_net_polarity(gt_tokens);
    let ma_pol = compute_net_polarity(ma_tokens);

    if gt_pol != 0 && ma_pol != 0 && gt_pol != ma_pol {
        return 0.25;
    }

    1.0
}

fn compute_net_polarity(tokens: &TokenList) -> i8 {
    let mut has_pos = false;
    let mut has_neg = false;
    let mut neg_count = 0usize;

    let mut i = 0;
    while i < tokens.count {
        let tok = tokens.get(i);

        let mut j = 0;
        while j < POSITIVE_WORDS.len() {
            if eq_ci(tok, POSITIVE_WORDS[j]) { has_pos = true; break; }
            j += 1;
        }

        j = 0;
        while j < NEGATIVE_WORDS.len() {
            if eq_ci(tok, NEGATIVE_WORDS[j]) { has_neg = true; break; }
            j += 1;
        }

        let mut found_neg = false;
        j = 0;
        while j < NEGATION_MODIFIERS.len() {
            if eq_ci(tok, NEGATION_MODIFIERS[j]) { found_neg = true; break; }
            j += 1;
        }
        if !found_neg && is_substring(b"n't", tok) { found_neg = true; }
        if found_neg { neg_count += 1; }

        i += 1;
    }

    let is_odd = (neg_count % 2) == 1;

    if has_pos && !is_odd { return 1; }
    if has_pos && is_odd { return -1; }
    if has_neg && !is_odd { return -1; }
    if has_neg && is_odd { return 1; }
    0
}

// ================================================================
//  9. CANONICAL FACT ENGINE
// ================================================================

fn number_word_to_digit(w: &[u8]) -> Option<&'static [u8]> {
    const WORDS: [(&[u8], &[u8]); 27] = [
        (b"one", b"1"), (b"first", b"1"), (b"two", b"2"), (b"second", b"2"),
        (b"three", b"3"), (b"third", b"3"), (b"four", b"4"), (b"fourth", b"4"),
        (b"five", b"5"), (b"fifth", b"5"), (b"six", b"6"), (b"sixth", b"6"),
        (b"seven", b"7"), (b"seventh", b"7"), (b"eight", b"8"), (b"eighth", b"8"),
        (b"nine", b"9"), (b"ninth", b"9"), (b"ten", b"10"), (b"tenth", b"10"),
        (b"eleven", b"11"), (b"eleventh", b"11"), (b"twelve", b"12"), (b"twelfth", b"12"),
        (b"twenty", b"20"), (b"twentieth", b"20"), (b"hundred", b"100"),
    ];
    let mut i = 0;
    while i < WORDS.len() {
        if eq_ci(w, WORDS[i].0) {
            return Some(WORDS[i].1);
        }
        i += 1;
    }
    None
}

fn canonicalize_fact<'a>(word: &[u8], buf: &'a mut [u8; NUM_BUF_LEN]) -> &'a [u8] {
    if word.len() >= 2 && word[0] == b'0' && (word[1] == b'x' || word[1] == b'X') {
        let m = word.len().min(NUM_BUF_LEN);
        let mut i = 0;
        while i < m { buf[i] = word[i].to_ascii_lowercase(); i += 1; }
        return &buf[..m];
    }

    if let Some(d) = number_word_to_digit(word) {
        let n = d.len().min(NUM_BUF_LEN);
        buf[..n].copy_from_slice(&d[..n]);
        return &buf[..n];
    }

    let wl = word.len();
    if wl >= 3 {
        let tail = &word[wl - 2..];
        if eq_ci(tail, b"st") || eq_ci(tail, b"nd") || eq_ci(tail, b"rd") || eq_ci(tail, b"th") {
            let head = &word[..wl - 2];
            if !head.is_empty() {
                let mut all_digit = true;
                let mut k = 0;
                while k < head.len() {
                    if !head[k].is_ascii_digit() { all_digit = false; break; }
                    k += 1;
                }
                if all_digit {
                    let m = head.len().min(NUM_BUF_LEN);
                    buf[..m].copy_from_slice(&head[..m]);
                    return &buf[..m];
                }
            }
        }
    }

    let mut has_digit = false;
    {
        let mut k = 0;
        while k < word.len() {
            if word[k].is_ascii_digit() { has_digit = true; break; }
            k += 1;
        }
    }
    if has_digit {
        let s = if word[0] == b'$' { 1 } else { 0 };
        let mut e = wl;
        let mut scale: u32 = 1;
        if e > s {
            match word[e - 1] {
                b'k' | b'K' => { scale = 1_000; e -= 1; }
                b'm' | b'M' => { scale = 1_000_000; e -= 1; }
                b'b' | b'B' => { scale = 1_000_000_000; e -= 1; }
                b'%' => { e -= 1; }
                _ => {}
            }
        }
        let mut has_dot = false;
        {
            let mut k = s;
            while k < e {
                if word[k] == b'.' { has_dot = true; break; }
                k += 1;
            }
        }
        if !(scale > 1 && has_dot) {
            let mut n = 0;
            let mut k = s;
            while k < e {
                if (word[k].is_ascii_digit() || word[k] == b'.') && n < NUM_BUF_LEN {
                    buf[n] = word[k];
                    n += 1;
                }
                k += 1;
            }
            if scale > 1 {
                let mut sc = scale;
                while sc > 1 {
                    if n < NUM_BUF_LEN { buf[n] = b'0'; n += 1; }
                    sc /= 10;
                }
            }
            if n > 0 { return &buf[..n]; }
        }
    }

    let m = wl.min(NUM_BUF_LEN);
    buf[..m].copy_from_slice(&word[..m]);
    &buf[..m]
}

fn is_fact_token(token: &[u8]) -> bool {
    if token.len() >= 2 && token[0] == b'0' && (token[1] == b'x' || token[1] == b'X') {
        return true;
    }
    if number_word_to_digit(token).is_some() {
        return true;
    }
    let mut i = 0;
    while i < token.len() {
        if token[i].is_ascii_digit() { return true; }
        i += 1;
    }
    false
}

/// Returns fact accuracy ratio: 1.0 if all facts match, smooth fraction otherwise.
fn fact_accuracy(gt_tokens: &TokenList, ma_tokens: &TokenList) -> f32 {
    let mut gt_buf = [0u8; NUM_BUF_LEN];
    let mut ma_buf = [0u8; NUM_BUF_LEN];

    let mut fact_count = 0usize;
    let mut matched_facts = 0usize;

    let mut i = 0;
    while i < gt_tokens.count {
        let gt_tok = gt_tokens.get(i);
        if !is_fact_token(gt_tok) { i += 1; continue; }

        fact_count += 1;
        let gt_canon = canonicalize_fact(gt_tok, &mut gt_buf);
        let mut matched = false;

        let mut j = 0;
        while j < ma_tokens.count {
            let ma_tok = ma_tokens.get(j);
            let ma_canon = canonicalize_fact(ma_tok, &mut ma_buf);
            if eq_ci(gt_canon, ma_canon) {
                matched = true;
                break;
            }
            j += 1;
        }

        if matched {
            matched_facts += 1;
        }
        i += 1;
    }

    if fact_count == 0 { 1.0 }
    else { matched_facts as f32 / fact_count as f32 }
}

// ================================================================
//  10. COMPOSITE SCORER — THE MAIN ENGINE
// ================================================================

fn evaluate(q_bytes: &[u8], gt_bytes: &[u8], ma_bytes: &[u8]) -> f32 {
    let mut q_tokens = TokenList::new();
    let mut gt_tokens = TokenList::new();
    let mut ma_tokens = TokenList::new();

    normalize_and_tokenize(q_bytes, &mut q_tokens);
    normalize_and_tokenize(gt_bytes, &mut gt_tokens);
    normalize_and_tokenize(ma_bytes, &mut ma_tokens);

    if gt_tokens.count == 0 {
        return if ma_tokens.count == 0 { 1.0 } else { 0.0 };
    }
    if ma_tokens.count == 0 {
        return 0.0;
    }

    // -- Signal 1: Correctness -- cosine(gt, ma) ------------------
    let gt_embed = embed_tokens(&gt_tokens);
    let ma_embed = embed_tokens(&ma_tokens);
    let correctness = cosine_similarity(&gt_embed, &ma_embed);

    // -- Signal 2: Relevance -- cosine(q, ma) ---------------------
    let relevance = if q_tokens.count > 0 {
        let q_embed = embed_tokens(&q_tokens);
        cosine_similarity(&q_embed, &ma_embed)
    } else {
        correctness
    };

    // -- Signal 3: BM25 Lexical Overlap ---------------------------
    let lexical = bm25_score(&gt_tokens, &ma_tokens);

    // -- Signal 4: Length Quality ---------------------------------
    let length = length_quality(&gt_tokens, &ma_tokens);

    // -- Composite Score ------------------------------------------
    let mut composite = W_CORRECTNESS * correctness
                      + W_RELEVANCE  * relevance
                      + W_LEXICAL    * lexical
                      + W_LENGTH     * length;

    // -- Polarity Penalty (soft) ----------------------------------
    let pol_mult = polarity_penalty(&gt_tokens, &ma_tokens);
    composite *= pol_mult;

    // -- Fact Accuracy Penalty ------------------------------------
    let fact_acc = fact_accuracy(&gt_tokens, &ma_tokens);
    if fact_acc < 1.0 {
        composite = composite * (0.7 + 0.3 * fact_acc);
    }

    // Clamp to [0, 1]
    if composite < 0.0 { 0.0 }
    else if composite > 1.0 { 1.0 }
    else { composite }
}

// ================================================================
//  11. WASM INTERFACE
// ================================================================

unsafe fn read_slice<'a>(ptr: i32, len: i32) -> &'a [u8] {
    if ptr == 0 || len <= 0 {
        &[]
    } else {
        core::slice::from_raw_parts(ptr as *const u8, len as usize)
    }
}

#[no_mangle]
pub unsafe extern "C" fn rank_answer(
    q_ptr: i32, q_len: i32,
    gt_ptr: i32, gt_len: i32,
    ma_ptr: i32, ma_len: i32,
) -> f32 {
    let q = read_slice(q_ptr, q_len);
    let gt = read_slice(gt_ptr, gt_len);
    let ma = read_slice(ma_ptr, ma_len);

    if ma.is_empty() { return 0.0; }
    if gt == ma { return 1.0; }

    evaluate(q, gt, ma)
}

static mut BUMP_HEAP: [u8; 64 * 1024] = [0u8; 64 * 1024];
static mut BUMP_OFFSET: usize = 0;

#[no_mangle]
pub unsafe extern "C" fn alloc(size: i32) -> i32 {
    let size = size.max(0) as usize;
    unsafe {
        let aligned = (BUMP_OFFSET + 7) & !7;
        if aligned + size > 64 * 1024 {
            BUMP_OFFSET = 0;
            core::ptr::addr_of_mut!(BUMP_HEAP).cast::<u8>() as i32
        } else {
            let ptr = core::ptr::addr_of_mut!(BUMP_HEAP).cast::<u8>().add(aligned);
            BUMP_OFFSET = aligned + size;
            ptr as i32
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn dealloc(_ptr: i32, _size: i32) {}
