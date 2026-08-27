const { keccak256 } = require('./node_modules/js-sha3');

const contractAddress = '0x5a2324aA18613FAD4e44bDF0d6c73Ec1f6D87ff8';
const rpcUrl = 'https://sepolia.base.org';

const signatures = [
    "registerMiner(bytes32,string,string)",
    "registerYaml(bytes32,string,string)",
    "registerYamlMiner(bytes32,string,string)",
    "registerEntry(bytes32,string,string)",
    "registerMiner(string,string,bytes32)",
    "register(bytes32,string,string)",
    "registerMinerEntry(bytes32,string,string)",
    "registerMinerEntry(string,string)",
    "registerYamlEntry(string,string)",
    "registerYamlEntry(bytes32,string,string)",
    "registerMiner(string,string,uint256,string)",
    "registerMiner(string,uint256,string)",
    "registerYaml(string,uint256)",
    "registerYaml(string,string,uint256)",
    "register(string,string,string)",
    "register(string,string,uint256)",
    "registerWasm(bytes32,string,string)"
];

async function check() {
    console.log("Checking function signatures batch 3...\n");
    for (const sig of signatures) {
        const selector = '0x' + keccak256(sig).slice(0, 8);
        
        const payload = {
            jsonrpc: "2.0",
            id: 1,
            method: "eth_call",
            params: [
                {
                    to: contractAddress,
                    data: selector
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
        
        const errStr = JSON.stringify(json);
        if (errStr.includes("Function does not exist")) {
            console.log(`[NOT EXIST] ${selector} -> ${sig}`);
        } else {
            console.log(`[MATCH FOUND!] Selector ${selector} -> ${sig}`);
        }
    }
}

check().catch(console.error);
