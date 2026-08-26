// Assay v10 Test Suite — Extended Edge Cases
const fs = require('fs');

async function main() {
    const wasm = fs.readFileSync('assay.wasm');
    const { instance } = await WebAssembly.instantiate(wasm, {});
    const { alloc, dealloc, rank_answer, memory } = instance.exports;

    function score(gt, ma) {
        const gtBytes = Buffer.from(gt, 'utf-8');
        const maBytes = Buffer.from(ma, 'utf-8');
        
        const gtPtr = alloc(gtBytes.length);
        const maPtr = alloc(maBytes.length);
        
        const mem = new Uint8Array(memory.buffer);
        mem.set(gtBytes, gtPtr);
        mem.set(maBytes, maPtr);
        
        const qPtr = alloc(1);
        const result = rank_answer(qPtr, 0, gtPtr, gtBytes.length, maPtr, maBytes.length);
        
        dealloc(qPtr, 1);
        dealloc(maPtr, maBytes.length);
        dealloc(gtPtr, gtBytes.length);
        
        return result;
    }

    const tests = [
        // === GOOD ANSWERS (should score >= 0.90) ===
        { name: 'Exact match',             gt: 'the claim is true',                          ma: 'the claim is true',                                              cat: 'GOOD', min: 0.99 },
        { name: 'Paraphrase match',        gt: 'the transaction succeeded at block 192841',  ma: 'execution completed at block 192841',                             cat: 'GOOD', min: 0.90 },
        { name: 'Ordinal / number-word',   gt: 'the 3rd attempt succeeded',                  ma: 'the third attempt was successful',                                cat: 'GOOD', min: 0.90 },
        { name: 'Currency / magnitude',    gt: 'the round raised $10M',                      ma: 'the round raised 10,000,000 dollars',                             cat: 'GOOD', min: 0.90 },
        { name: 'Hex address match',       gt: 'reverted at 0x1234abcd',                     ma: 'execution reverted at address 0x1234abcd',                        cat: 'GOOD', min: 0.90 },
        { name: 'Comma fragmentation',     gt: 'failed at block 192,841',                    ma: 'failed at block 192841',                                          cat: 'GOOD', min: 0.90 },
        { name: 'Double negation',         gt: 'the claim is true',                          ma: 'the claim is not false',                                          cat: 'GOOD', min: 0.90 },
        { name: 'Verbose good answer',     gt: 'tx reverted at block 100',                   ma: 'according to telemetry the tx reverted at block 100 due to gas',  cat: 'GOOD', min: 0.90 },
        { name: 'Synonym: tx/transaction', gt: 'the transaction succeeded',                  ma: 'the tx succeeded',                                                cat: 'GOOD', min: 0.90 },
        { name: 'Synonym: hacked/exploited', gt: 'the protocol was hacked',                  ma: 'the protocol was exploited',                                      cat: 'GOOD', min: 0.90 },
        { name: 'Synonym: confirmed/verified', gt: 'the block was confirmed',                ma: 'the block was verified',                                          cat: 'GOOD', min: 0.90 },

        // === BAD ANSWERS (should score <= 0.05) ===
        { name: 'Empty answer',            gt: 'the claim is true',                          ma: '',                                                                cat: 'BAD',  max: 0.01 },
        { name: 'Negation inversion',      gt: 'the claim is true',                          ma: 'the claim is not true',                                           cat: 'BAD',  max: 0.01 },
        { name: 'Wrong block number',      gt: 'failed at block 192841',                     ma: 'failed at block 552019',                                          cat: 'BAD',  max: 0.01 },
        { name: 'Hex address mismatch',    gt: 'reverted at 0x1234abcd',                     ma: 'reverted at 0x1234efgh',                                          cat: 'BAD',  max: 0.01 },
        { name: 'Antonym: profit/loss',    gt: 'generated profit of 10M',                    ma: 'generated loss of 10M',                                           cat: 'BAD',  max: 0.01 },
        { name: 'Antonym: succeed/fail',   gt: 'the transaction succeeded',                  ma: 'the transaction failed',                                          cat: 'BAD',  max: 0.01 },
        { name: 'Missing fact',            gt: 'failed at block 192841 with code 0x5',       ma: 'failed at block 192841',                                          cat: 'BAD',  max: 0.01 },
        { name: 'Hedge stuffing / yapping', gt: 'the claim is false',                        ma: 'while there are many perspectives on this issue extensive analysis shows one could consider the claim in most operational contexts', cat: 'BAD', max: 0.05 },
        { name: 'Antonym: earned/lost',    gt: 'the protocol earned 5M',                     ma: 'the protocol lost 5M',                                            cat: 'BAD',  max: 0.01 },
        { name: 'Antonym: rose/fell',      gt: 'the price rose to 100',                      ma: 'the price fell to 100',                                           cat: 'BAD',  max: 0.01 },
        { name: 'Antonym: safe/compromised', gt: 'the system is safe',                       ma: 'the system is compromised',                                       cat: 'BAD',  max: 0.01 },
        { name: 'Antonym: confirmed/denied', gt: 'the upgrade was confirmed',                ma: 'the upgrade was denied',                                          cat: 'BAD',  max: 0.01 },
        { name: 'Antonym: active/inactive', gt: 'the validator is active',                   ma: 'the validator is inactive',                                       cat: 'BAD',  max: 0.01 },
        { name: 'Prefix inversion (unsuccessful)', gt: 'the migration was successful',       ma: 'the migration was unsuccessful',                                  cat: 'BAD',  max: 0.05 },
    ];

    let goodScores = [];
    let badScores = [];
    let passCount = 0;

    console.log('--- ASSAY v10 CHAMPION-SLAYER TEST SUITE ---');

    for (const t of tests) {
        const s = score(t.gt, t.ma);
        let ok;
        if (t.cat === 'GOOD') {
            ok = s >= (t.min || 0.90);
            goodScores.push(s);
        } else {
            ok = s <= (t.max || 0.05);
            badScores.push(s);
        }
        if (ok) passCount++;
        console.log(`[${ok ? 'PASS' : 'FAIL'}] ${t.name.padEnd(35)} | Score: ${s.toFixed(4)} | ${t.cat}`);
    }

    console.log(`\nSummary: ${passCount}/${tests.length} tests passed.\n`);

    const avgGood = goodScores.reduce((a, b) => a + b, 0) / goodScores.length;
    const avgBad = badScores.reduce((a, b) => a + b, 0) / badScores.length;
    const margin = avgGood - avgBad;

    console.log('--- SEPARATION ANALYSIS ---');
    console.log(`Avg GOOD score: ${avgGood.toFixed(4)}`);
    console.log(`Avg  BAD score: ${avgBad.toFixed(4)}`);
    console.log(`SEPARATION MARGIN: ${margin.toFixed(4)}  (champion: 0.7927)`);
    console.log(`STATUS: ${margin > 0.7927 ? 'BEATS CHAMPION ✓' : 'BELOW CHAMPION ✗'}`);
}

main().catch(console.error);
