#![no_std]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

// ================================================================
//  ASSAY v3 — Maximum-Separation Scoring Engine
//
//  Design Philosophy:
//  The validator measures SEPARATION = avg(good_scores) - avg(bad_scores).
//  Champion margin is 0.7916. To make this nearly unbeatable we need:
//    • Good answers → 0.95 - 1.00  (ceiling)
//    • Bad answers  → 0.00 - 0.02  (floor)
//    • Separation   → ~0.95+
//
//  Key innovations over v2:
//  1. Steep smoothstep sigmoid curve (replaces linear piecewise)
//  2. Synonym matching for fact-check domain vocabulary
//  3. Suffix-stripping stemmer for paraphrase robustness
//  4. Cubic fact gate (any missing fact → near-zero)
//  5. Substring containment boost (GT inside MA → high score)
//  6. Ultra-harsh negation gate (0.01x multiplier)
// ================================================================

// ---------- Memory: bump allocator ----------
const HEAP_SIZE: usize = 1 * 1024 * 1024;
static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
static mut HEAP_OFFSET: usize = 0;

#[no_mangle]
pub unsafe extern "C" fn alloc(size: i32) -> i32 {
    let size = size.max(0) as usize;
    unsafe {
        let aligned = (HEAP_OFFSET + 3) & !3;
        if aligned + size > HEAP_SIZE {
            HEAP_OFFSET = 0;
        } else {
            HEAP_OFFSET = aligned;
        }
        let ptr = core::ptr::addr_of_mut!(HEAP).cast::<u8>().add(HEAP_OFFSET);
        HEAP_OFFSET += size;
        ptr as i32
    }
}

#[no_mangle]
pub unsafe extern "C" fn dealloc(_ptr: i32, _size: i32) {}

unsafe fn read_str<'a>(ptr: i32, len: i32) -> &'a str {
    unsafe {
        let slice = core::slice::from_raw_parts(ptr as *const u8, len.max(0) as usize);
        core::str::from_utf8(slice).unwrap_or("")
    }
}

// ---------- Tokenizer ----------
#[derive(Clone, Copy)]
struct TokenSpan {
    start: usize,
    end: usize,
}

const MAX_TOKENS: usize = 128;

fn is_delim_at(input: &[u8], i: usize) -> bool {
    let b = input[i];
    match b {
        b' ' | b'\n' | b'\r' | b'\t' | b'!' | b'?' | b';' | b':' | b'"' | b'(' | b')'
        | b'[' | b']' | b'{' | b'}' | b'<' | b'>' | b'/' | b'\\' | b'\'' => true,
        b',' | b'.' => {
            let prev_digit = i > 0 && input[i - 1].is_ascii_digit();
            let next_digit = i + 1 < input.len() && input[i + 1].is_ascii_digit();
            !(prev_digit && next_digit)
        }
        _ => false,
    }
}

fn tokenize(input: &[u8], out: &mut [TokenSpan; MAX_TOKENS]) -> usize {
    let mut count = 0;
    let mut in_token = false;
    let mut start = 0;
    for i in 0..input.len() {
        let d = is_delim_at(input, i);
        if !d && !in_token {
            in_token = true;
            start = i;
        } else if d && in_token {
            in_token = false;
            if count < MAX_TOKENS {
                out[count] = TokenSpan { start, end: i };
                count += 1;
            } else {
                break;
            }
        }
    }
    if in_token && count < MAX_TOKENS {
        out[count] = TokenSpan { start, end: input.len() };
        count += 1;
    }
    count
}

fn eq_ignore_case(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for i in 0..a.len() {
        if a[i].to_ascii_lowercase() != b[i].to_ascii_lowercase() {
            return false;
        }
    }
    true
}

// ---------- Stopwords (expanded) ----------
const STOPWORDS: [&[u8]; 30] = [
    b"the", b"a", b"an", b"is", b"was", b"are", b"were", b"of", b"to", b"in",
    b"on", b"at", b"and", b"or", b"it", b"this", b"that", b"be", b"as", b"by",
    b"for", b"with", b"has", b"had", b"have", b"its", b"from", b"been", b"being", b"do",
];

fn is_stopword(word: &[u8]) -> bool {
    for sw in STOPWORDS.iter() {
        if eq_ignore_case(word, sw) {
            return true;
        }
    }
    false
}

// ---------- Synonym Groups for FACT_CHECK domain ----------
// Returns a nonzero group ID if the word belongs to a synonym cluster.
// Words with the same group ID are treated as equivalent.
fn synonym_group(word: &[u8]) -> u8 {
    // Group 1: truth / correctness
    const G1: [&[u8]; 8] = [b"true", b"correct", b"accurate", b"valid", b"verified", b"confirmed", b"right", b"yes"];
    // Group 2: falsehood / incorrectness
    const G2: [&[u8]; 8] = [b"false", b"incorrect", b"wrong", b"untrue", b"inaccurate", b"invalid", b"disproven", b"debunked"];
    // Group 3: increase / growth
    const G3: [&[u8]; 6] = [b"increase", b"rise", b"grow", b"gain", b"surge", b"jump"];
    // Group 4: decrease / decline
    const G4: [&[u8]; 6] = [b"decrease", b"fall", b"drop", b"decline", b"plunge", b"sink"];
    // Group 5: transaction terms
    const G5: [&[u8]; 4] = [b"transaction", b"tx", b"txn", b"transfer"];
    // Group 6: failure terms
    const G6: [&[u8]; 4] = [b"failed", b"reverted", b"rejected", b"aborted"];
    // Group 7: success terms
    const G7: [&[u8]; 4] = [b"succeeded", b"passed", b"confirmed", b"completed"];
    // Group 8: execution terms
    const G8: [&[u8]; 3] = [b"execution", b"processing", b"running"];

    for w in G1.iter() { if eq_ignore_case(word, w) { return 1; } }
    for w in G2.iter() { if eq_ignore_case(word, w) { return 2; } }
    for w in G3.iter() { if eq_ignore_case(word, w) { return 3; } }
    for w in G4.iter() { if eq_ignore_case(word, w) { return 4; } }
    for w in G5.iter() { if eq_ignore_case(word, w) { return 5; } }
    for w in G6.iter() { if eq_ignore_case(word, w) { return 6; } }
    for w in G7.iter() { if eq_ignore_case(word, w) { return 7; } }
    for w in G8.iter() { if eq_ignore_case(word, w) { return 8; } }
    0
}

// ---------- Suffix-Stripping Stemmer ----------
const STEM_BUF_LEN: usize = 64;

fn stem_word<'a>(word: &[u8], buf: &'a mut [u8; STEM_BUF_LEN]) -> &'a [u8] {
    let len = word.len().min(STEM_BUF_LEN);
    for i in 0..len {
        buf[i] = word[i].to_ascii_lowercase();
    }
    let mut n = len;

    // Strip common English suffixes (longest first)
    if n > 6 && buf[n-4] == b'm' && buf[n-3] == b'e' && buf[n-2] == b'n' && buf[n-1] == b't' {
        n -= 4; // -ment
    } else if n > 6 && buf[n-4] == b'n' && buf[n-3] == b'e' && buf[n-2] == b's' && buf[n-1] == b's' {
        n -= 4; // -ness
    } else if n > 5 && buf[n-3] == b'i' && buf[n-2] == b'n' && buf[n-1] == b'g' {
        n -= 3; // -ing
    } else if n > 5 && buf[n-3] == b'i' && buf[n-2] == b'o' && buf[n-1] == b'n' {
        n -= 3; // -ion
    } else if n > 5 && buf[n-3] == b'o' && buf[n-2] == b'u' && buf[n-1] == b's' {
        n -= 3; // -ous
    } else if n > 5 && buf[n-3] == b'i' && buf[n-2] == b'v' && buf[n-1] == b'e' {
        n -= 3; // -ive
    } else if n > 4 && buf[n-2] == b'e' && buf[n-1] == b'd' {
        n -= 2; // -ed
    } else if n > 4 && buf[n-2] == b'l' && buf[n-1] == b'y' {
        n -= 2; // -ly
    } else if n > 4 && buf[n-2] == b'e' && buf[n-1] == b'r' {
        n -= 2; // -er
    } else if n > 4 && buf[n-2] == b'a' && buf[n-1] == b'l' {
        n -= 2; // -al
    } else if n > 3 && buf[n-1] == b's' && buf[n-2] != b's' {
        n -= 1; // -s (not -ss)
    }

    &buf[..n]
}

// ---------- Fact extraction (numbers, addresses, number-words, currency) ----------
fn is_fact_token(word: &[u8]) -> bool {
    if word.len() >= 2 && word[0] == b'0' && (word[1] == b'x' || word[1] == b'X') {
        return true;
    }
    word.iter().any(|b| b.is_ascii_digit())
}

fn number_word_to_digits(word: &[u8]) -> Option<&'static [u8]> {
    const WORDS: [(&[u8], &[u8]); 27] = [
        (b"one", b"1"), (b"first", b"1"),
        (b"two", b"2"), (b"second", b"2"),
        (b"three", b"3"), (b"third", b"3"),
        (b"four", b"4"), (b"fourth", b"4"),
        (b"five", b"5"), (b"fifth", b"5"),
        (b"six", b"6"), (b"sixth", b"6"),
        (b"seven", b"7"), (b"seventh", b"7"),
        (b"eight", b"8"), (b"eighth", b"8"),
        (b"nine", b"9"), (b"ninth", b"9"),
        (b"ten", b"10"), (b"tenth", b"10"),
        (b"eleven", b"11"), (b"eleventh", b"11"),
        (b"twelve", b"12"), (b"twelfth", b"12"),
        (b"twenty", b"20"), (b"twentieth", b"20"),
        (b"hundred", b"100"),
    ];
    for (w, d) in WORDS.iter() {
        if eq_ignore_case(word, w) {
            return Some(d);
        }
    }
    None
}

fn is_fact_or_number_word(word: &[u8]) -> bool {
    is_fact_token(word) || number_word_to_digits(word).is_some()
}

const NUM_BUF_LEN: usize = 64;

fn canonicalize<'a>(word: &[u8], buf: &'a mut [u8; NUM_BUF_LEN]) -> &'a [u8] {
    // Hex addresses: lowercase
    if word.len() >= 2 && word[0] == b'0' && (word[1] == b'x' || word[1] == b'X') {
        let m = word.len().min(NUM_BUF_LEN);
        for i in 0..m {
            buf[i] = word[i].to_ascii_lowercase();
        }
        return &buf[..m];
    }

    // Number words → digit strings
    if let Some(d) = number_word_to_digits(word) {
        let n = d.len().min(NUM_BUF_LEN);
        buf[..n].copy_from_slice(&d[..n]);
        return &buf[..n];
    }

    // Ordinals: 3rd → 3, 1st → 1
    let wlen = word.len();
    if wlen >= 3 {
        let tail = &word[wlen - 2..];
        if eq_ignore_case(tail, b"st")
            || eq_ignore_case(tail, b"nd")
            || eq_ignore_case(tail, b"rd")
            || eq_ignore_case(tail, b"th")
        {
            let head = &word[..wlen - 2];
            if !head.is_empty() && head.iter().all(|b| b.is_ascii_digit()) {
                let m = head.len().min(NUM_BUF_LEN);
                buf[..m].copy_from_slice(&head[..m]);
                return &buf[..m];
            }
        }
    }

    // Currency/magnitude: $10M → 10000000
    if word.iter().any(|b| b.is_ascii_digit()) {
        let mut start = 0usize;
        if start < word.len() && word[start] == b'$' {
            start += 1;
        }
        let mut end = word.len();
        let mut suffix_scale: u32 = 1;
        if end > start {
            match word[end - 1] {
                b'%' => { end -= 1; }
                b'k' | b'K' => { suffix_scale = 1_000; end -= 1; }
                b'm' | b'M' => { suffix_scale = 1_000_000; end -= 1; }
                b'b' | b'B' => { suffix_scale = 1_000_000_000; end -= 1; }
                _ => {}
            }
        }
        let has_decimal_point = start < end && word[start..end].iter().any(|&b| b == b'.');

        if !(suffix_scale > 1 && has_decimal_point) {
            let mut n = 0usize;
            for i in start..end {
                let b = word[i];
                if (b.is_ascii_digit() || b == b'.') && n < NUM_BUF_LEN {
                    buf[n] = b;
                    n += 1;
                }
            }
            if suffix_scale > 1 {
                let mut scale = suffix_scale;
                while scale > 1 {
                    if n < NUM_BUF_LEN {
                        buf[n] = b'0';
                        n += 1;
                    }
                    scale /= 10;
                }
            }
            if n > 0 {
                return &buf[..n];
            }
        }
    }

    let m = word.len().min(NUM_BUF_LEN);
    buf[..m].copy_from_slice(&word[..m]);
    &buf[..m]
}

// ---------- Multi-layer word equivalence ----------
// Two words match if ANY of these is true:
//   1. Case-insensitive exact match
//   2. Canonical forms match (numbers, currency, ordinals, hex)
//   3. Same synonym group (fact-check domain vocabulary)
//   4. Stems match (suffix-stripped forms)
fn eq_words(a: &[u8], b: &[u8]) -> bool {
    // Layer 1: exact case-insensitive
    if eq_ignore_case(a, b) {
        return true;
    }
    // Layer 2: canonical form (numbers, hex, currency)
    let mut buf_a = [0u8; NUM_BUF_LEN];
    let mut buf_b = [0u8; NUM_BUF_LEN];
    let ca = canonicalize(a, &mut buf_a);
    let cb = canonicalize(b, &mut buf_b);
    if eq_ignore_case(ca, cb) {
        return true;
    }
    // Layer 3: synonym groups
    let ga = synonym_group(a);
    if ga != 0 && ga == synonym_group(b) {
        return true;
    }
    // Layer 4: stem matching
    let mut stem_a = [0u8; STEM_BUF_LEN];
    let mut stem_b = [0u8; STEM_BUF_LEN];
    let sa = stem_word(a, &mut stem_a);
    let sb = stem_word(b, &mut stem_b);
    if sa.len() >= 3 && sa == sb {
        return true;
    }
    false
}

fn facts_match(
    gt: &[u8], gt_tokens: &[TokenSpan],
    ma: &[u8], ma_tokens: &[TokenSpan],
) -> (usize, usize) {
    let mut total = 0usize;
    let mut matched = 0usize;
    let mut gt_buf = [0u8; NUM_BUF_LEN];
    let mut ma_buf = [0u8; NUM_BUF_LEN];
    for gt_span in gt_tokens {
        let gt_word = &gt[gt_span.start..gt_span.end];
        if !is_fact_or_number_word(gt_word) {
            continue;
        }
        total += 1;
        let gt_canon = canonicalize(gt_word, &mut gt_buf);
        for ma_span in ma_tokens {
            let ma_word = &ma[ma_span.start..ma_span.end];
            let ma_canon = canonicalize(ma_word, &mut ma_buf);
            if eq_ignore_case(gt_canon, ma_canon) {
                matched += 1;
                break;
            }
        }
    }
    (matched, total)
}

// ---------- Content words (stopwords removed) ----------
fn content_words(
    text: &[u8],
    tokens: &[TokenSpan],
    count: usize,
    out: &mut [(usize, usize); MAX_TOKENS],
) -> usize {
    let mut n = 0;
    for i in 0..count {
        let span = tokens[i];
        let word = &text[span.start..span.end];
        if !is_stopword(word) && n < MAX_TOKENS {
            out[n] = (span.start, span.end);
            n += 1;
        }
    }
    n
}

// ---------- Recall Metrics ----------
fn unigram_recall(
    text_a: &[u8], words_a: &[(usize, usize)], na: usize,
    text_b: &[u8], words_b: &[(usize, usize)], nb: usize,
) -> f32 {
    if na == 0 { return 1.0; }
    if nb == 0 { return 0.0; }
    let mut matched = 0usize;
    for i in 0..na {
        let (s, e) = words_a[i];
        let wa = &text_a[s..e];
        for j in 0..nb {
            let (s2, e2) = words_b[j];
            let wb = &text_b[s2..e2];
            if eq_words(wa, wb) {
                matched += 1;
                break;
            }
        }
    }
    matched as f32 / na as f32
}

fn bigram_recall(
    text_a: &[u8], words_a: &[(usize, usize)], na: usize,
    text_b: &[u8], words_b: &[(usize, usize)], nb: usize,
) -> f32 {
    if na < 2 {
        return unigram_recall(text_a, words_a, na, text_b, words_b, nb);
    }
    let bigrams_a = na - 1;
    let mut matched = 0usize;
    for i in 0..bigrams_a {
        let (a1s, a1e) = words_a[i];
        let (a2s, a2e) = words_a[i + 1];
        let w1a = &text_a[a1s..a1e];
        let w2a = &text_a[a2s..a2e];
        for j in 0..nb.saturating_sub(1) {
            let (b1s, b1e) = words_b[j];
            let (b2s, b2e) = words_b[j + 1];
            let w1b = &text_b[b1s..b1e];
            let w2b = &text_b[b2s..b2e];
            if eq_words(w1a, w1b) && eq_words(w2a, w2b) {
                matched += 1;
                break;
            }
        }
    }
    matched as f32 / bigrams_a as f32
}

const LCS_CAP: usize = 64;

fn lcs_recall(
    text_a: &[u8], words_a: &[(usize, usize)], na: usize,
    text_b: &[u8], words_b: &[(usize, usize)], nb: usize,
) -> f32 {
    let na = na.min(LCS_CAP);
    let nb = nb.min(LCS_CAP);
    if na == 0 { return 1.0; }
    if nb == 0 { return 0.0; }

    let mut dp = [[0u16; LCS_CAP + 1]; LCS_CAP + 1];
    for i in 1..=na {
        let (s, e) = words_a[i - 1];
        let wa = &text_a[s..e];
        for j in 1..=nb {
            let (s2, e2) = words_b[j - 1];
            let wb = &text_b[s2..e2];
            if eq_words(wa, wb) {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = if dp[i - 1][j] > dp[i][j - 1] {
                    dp[i - 1][j]
                } else {
                    dp[i][j - 1]
                };
            }
        }
    }
    dp[na][nb] as f32 / na as f32
}

// ---------- Substring containment check ----------
// If GT is contained byte-for-byte (case-insensitive) inside MA,
// the answer almost certainly includes the correct fact.
fn contains_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    let limit = haystack.len() - needle.len();
    let mut i = 0;
    while i <= limit {
        let mut ok = true;
        for j in 0..needle.len() {
            if haystack[i + j].to_ascii_lowercase() != needle[j].to_ascii_lowercase() {
                ok = false;
                break;
            }
        }
        if ok {
            return true;
        }
        i += 1;
    }
    false
}

// ---------- Negation asymmetry ----------
fn negation_count(text: &[u8], words: &[(usize, usize)], n: usize) -> u32 {
    const NEG: [&[u8]; 12] = [
        b"not", b"never", b"no", b"cannot", b"false", b"untrue",
        b"incorrect", b"denies", b"refutes", b"debunked",
        b"wrong", b"inaccurate",
    ];
    let mut count = 0u32;
    for i in 0..n {
        let (s, e) = words[i];
        let w = &text[s..e];
        for neg in NEG.iter() {
            if eq_ignore_case(w, neg) {
                count += 1;
                break;
            }
        }
        // Detect contractions: doesn't, isn't, wasn't, etc.
        if w.len() >= 3 {
            let tail = &w[w.len() - 3..];
            if eq_ignore_case(tail, b"n't") || eq_ignore_case(tail, b"nt'") {
                count += 1;
            }
        }
    }
    count
}

// ---------- Length-ratio penalty (anti hedge-stuffing) ----------
// More aggressive: uses gt*1.5 instead of gt*2.0, and squares the ratio
fn length_penalty(gt_len: usize, ma_len: usize) -> f32 {
    if ma_len == 0 { return 0.0; }
    let ratio = (gt_len as f32 * 1.5) / (ma_len as f32);
    if ratio >= 1.0 {
        1.0
    } else {
        // Square the ratio for harsher penalty on verbose answers
        ratio * ratio
    }
}

// ================================================================
//  STEEP SMOOTHSTEP SEPARATION CURVE
//
//  This is the heart of the separation engine.
//  Uses a cubic Hermite smoothstep to create a near-binary classifier:
//
//  Input range [0.00 .. 0.20] → Output [0.000 .. 0.005]  (BAD: crushed)
//  Input range [0.20 .. 0.55] → Output [0.005 .. 0.950]  (transition)
//  Input range [0.55 .. 1.00] → Output [0.950 .. 1.000]  (GOOD: boosted)
//
//  Smoothstep(t) = t² × (3 − 2t) — no discontinuities, fully differentiable
// ================================================================
fn separation_curve(x: f32) -> f32 {
    if x >= 0.55 {
        // Good zone: saturate near 1.0
        let t = ((x - 0.55) / 0.45).min(1.0);
        0.95 + 0.05 * t
    } else if x <= 0.20 {
        // Bad zone: crush toward 0.0
        x * 0.025
    } else {
        // Transition zone: steep smoothstep
        let t = (x - 0.20) / 0.35;
        let s = t * t * (3.0 - 2.0 * t); // cubic smoothstep
        0.005 + 0.945 * s
    }
}

// ---------- Final scoring engine ----------
fn score(_question: &str, ground_truth: &str, miner_answer: &str) -> f32 {
    let gt_bytes = ground_truth.as_bytes();
    let ma_bytes = miner_answer.as_bytes();

    let mut gt_tok = [TokenSpan { start: 0, end: 0 }; MAX_TOKENS];
    let mut ma_tok = [TokenSpan { start: 0, end: 0 }; MAX_TOKENS];
    let gt_count = tokenize(gt_bytes, &mut gt_tok);
    let ma_count = tokenize(ma_bytes, &mut ma_tok);

    if ma_count == 0 {
        return 0.0;
    }

    // ---- Layer 1: Fact gate (cubic — any missing fact is devastating) ----
    let (matched_facts, total_facts) =
        facts_match(gt_bytes, &gt_tok[..gt_count], ma_bytes, &ma_tok[..ma_count]);
    let fact_ratio: f32 = if total_facts == 0 {
        1.0
    } else {
        matched_facts as f32 / total_facts as f32
    };
    // Cube the ratio: 1 missing fact out of 2 → 0.5^3 = 0.125 instead of 0.5
    let fact_gate = fact_ratio * fact_ratio * fact_ratio;

    // ---- Layer 2: Content word extraction ----
    let mut gt_content = [(0usize, 0usize); MAX_TOKENS];
    let mut ma_content = [(0usize, 0usize); MAX_TOKENS];
    let gt_c_count = content_words(gt_bytes, &gt_tok, gt_count, &mut gt_content);
    let ma_c_count = content_words(ma_bytes, &ma_tok, ma_count, &mut ma_content);

    // ---- Layer 3: Multi-metric recall (with stems + synonyms via eq_words) ----
    let unigram_s = unigram_recall(
        gt_bytes, &gt_content, gt_c_count,
        ma_bytes, &ma_content, ma_c_count,
    );
    let bigram_s = bigram_recall(
        gt_bytes, &gt_content, gt_c_count,
        ma_bytes, &ma_content, ma_c_count,
    );
    let lcs_s = lcs_recall(
        gt_bytes, &gt_content, gt_c_count,
        ma_bytes, &ma_content, ma_c_count,
    );

    // Weighted combination: LCS (order-sensitive) > unigram (coverage) > bigram (adjacency)
    let raw_recall = (lcs_s * 0.40) + (unigram_s * 0.35) + (bigram_s * 0.25);

    // ---- Layer 4: Substring containment boost ----
    // If the entire GT text appears inside MA, the answer is almost certainly correct.
    // Boost raw_recall to at least 0.60.
    let boosted_recall = if contains_case_insensitive(ma_bytes, gt_bytes) {
        raw_recall.max(0.60)
    } else {
        raw_recall
    };

    // ---- Layer 5: Negation gate (ultra-harsh) ----
    let neg_gt = negation_count(gt_bytes, &gt_content, gt_c_count);
    let neg_ma = negation_count(ma_bytes, &ma_content, ma_c_count);
    let negation_gate: f32 = if neg_gt != neg_ma { 0.01 } else { 1.0 };

    // ---- Layer 6: Length penalty (anti hedge-stuffing, squared) ----
    let len_penalty = length_penalty(gt_bytes.len(), ma_bytes.len());

    // ---- Layer 7: Steep smoothstep separation curve ----
    let similarity = separation_curve(boosted_recall);

    // ---- Combine all gates ----
    let combined = similarity * fact_gate * negation_gate * len_penalty;

    combined.clamp(0.0, 1.0)
}

#[no_mangle]
pub unsafe extern "C" fn rank_answer(
    q_ptr: i32, q_len: i32,
    gt_ptr: i32, gt_len: i32,
    ma_ptr: i32, ma_len: i32,
) -> f32 {
    unsafe {
        let question = read_str(q_ptr, q_len);
        let ground_truth = read_str(gt_ptr, gt_len);
        let miner_answer = read_str(ma_ptr, ma_len);

        if miner_answer.trim().is_empty() {
            return 0.0;
        }
        if miner_answer == ground_truth {
            return 1.0;
        }

        score(question, ground_truth, miner_answer)
    }
}
