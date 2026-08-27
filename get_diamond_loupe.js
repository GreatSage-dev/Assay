const { keccak256 } = require('./node_modules/js-sha3');

const contractAddress = '0x5a2324aA18613FAD4e44bDF0d6c73Ec1f6D87ff8';
const rpcUrl = 'https://sepolia.base.org';

async function getFacets() {
    console.log("Querying Diamond Loupe facets() on contract 0x5a2324aA...\n");
    
    // facets() selector: 0x7a0cd22c
    const payload = {
        jsonrpc: "2.0",
        id: 1,
        method: "eth_call",
        params: [
            {
                to: contractAddress,
                data: "0x7a0cd22c"
            },
            "latest"
        ]
    };

    const res = await fetch(rpcUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
    });
    const json = await res.json();
    console.log("Facets raw response:", JSON.stringify(json, null, 2));
}

getFacets().catch(console.error);
