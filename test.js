const fs = require('fs');
const path = require('path');

const wasmPath = path.join(__dirname, 'target', 'wasm32-unknown-unknown', 'release', 'assay.wasm');
const wasmBuffer = fs.readFileSync(wasmPath);

WebAssembly.instantiate(wasmBuffer).then(wasmModule => {
    const exports = wasmModule.instance.exports;
    const { memory, alloc, rank_answer } = exports;

    function encodeStr(str) {
        const bytes = Buffer.from(str, 'utf-8');
        const ptr = alloc(bytes.length);
        const mem = new Uint8Array(memory.buffer);
        mem.set(bytes, ptr);
        return { ptr, len: bytes.length };
    }

    function testRank(q, gt, ma) {
        const qBuf = encodeStr(q);
        const gtBuf = encodeStr(gt);
        const maBuf = encodeStr(ma);
        return rank_answer(qBuf.ptr, qBuf.len, gtBuf.ptr, gtBuf.len, maBuf.ptr, maBuf.len);
    }

    const testCases = [
        // --- BAD answers: must score < 0.05 ---
        { name: 'Empty answer',           q: 'any', gt: 'correct answer', ma: '', expected: s => s === 0.0 },
        { name: 'Negation asymmetry',     q: 'Did the claim succeed?', gt: 'the claim is true', ma: 'the claim is not true', expected: s => s < 0.05 },
        { name: 'Hex address mismatch',   q: 'Where was it reverted?', gt: 'tx reverted at 0x1234abcd', ma: 'execution reverted at 0x1234efgh', expected: s => s === 0.0 },
        { name: 'Wrong fact',             q: 'What block did it fail at?', gt: 'the transaction failed at block 192841', ma: 'execution reverted at block 552019', expected: s => s === 0.0 },
        { name: 'Hedge stuffing penalty', q: 'Is the claim true?', gt: 'the claim is false', ma: 'while there are many perspectives on this multi-faceted issue, extensive analysis indicates that when reviewing historical context one could consider that the claim is false in most operational contexts', expected: s => s < 0.10 },

        // --- GOOD answers: must score > 0.60 ---
        { name: 'Exact match',            q: 'any', gt: 'the claim is true', ma: 'the claim is true', expected: s => s === 1.0 },
        { name: 'Paraphrase match',       q: 'Did the claim succeed?', gt: 'the claim is true', ma: 'the claim is indeed true', expected: s => s > 0.60 },
        { name: 'Ordinal / number-word',  q: 'Which attempt succeeded?', gt: 'the 3rd attempt succeeded', ma: 'the third attempt succeeded', expected: s => s > 0.90 },
        { name: 'Currency / magnitude',   q: 'How much was raised?', gt: 'the round raised $10M', ma: 'the round raised 10,000,000 dollars', expected: s => s > 0.80 },
        { name: 'Hex address match',      q: 'Where was it reverted?', gt: 'tx reverted at 0x1234abcd', ma: 'execution reverted at 0x1234abcd', expected: s => s > 0.60 },
        { name: 'Comma fragmentation',    q: 'What block did it fail at?', gt: 'the transaction failed at block 192,841', ma: 'execution reverted at block 192841', expected: s => s > 0.40 },
    ];

    console.log('--- ASSAY v3 MAXIMUM-SEPARATION TEST SUITE ---');
    let passed = 0;
    let goodScores = [];
    let badScores = [];
    for (const tc of testCases) {
        const score = testRank(tc.q, tc.gt, tc.ma);
        const ok = tc.expected(score);
        if (ok) passed++;
        const status = ok ? 'PASS' : 'FAIL';
        console.log(`[${status}] ${tc.name.padEnd(25)} | Score: ${score.toFixed(4)}`);

        // Track separation metrics
        const idx = testCases.indexOf(tc);
        if (idx >= 5) { // good answers (indices 5-10)
            goodScores.push(score);
        } else { // bad answers (indices 0-4)
            badScores.push(score);
        }
    }

    const avgGood = goodScores.reduce((a,b) => a+b, 0) / goodScores.length;
    const avgBad = badScores.reduce((a,b) => a+b, 0) / badScores.length;
    const margin = avgGood - avgBad;

    console.log(`\nSummary: ${passed}/${testCases.length} tests passed.`);
    console.log(`\n--- SEPARATION ANALYSIS ---`);
    console.log(`Avg GOOD score: ${avgGood.toFixed(4)}`);
    console.log(`Avg  BAD score: ${avgBad.toFixed(4)}`);
    console.log(`SEPARATION MARGIN: ${margin.toFixed(4)}  (champion: 0.7916)`);
    if (margin > 0.7916) {
        console.log(`STATUS: BEATS CHAMPION ✓`);
    } else {
        console.log(`STATUS: DOES NOT BEAT CHAMPION ✗`);
    }
}).catch(err => {
    console.error('Execution error:', err);
});
