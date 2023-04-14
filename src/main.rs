use bitcoin::consensus::encode::deserialize;
use bitcoin::util::psbt::PartiallySignedTransaction;
use bitcoin::Address;
use bitcoin::Network;
use serde_json::json;

pub fn parse_psbt(base64_psbt: &str, network: Option<Network>) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let network = network.unwrap_or(Network::Bitcoin);

    // Decode the base64 PSBT
    let decoded_psbt = base64::decode(base64_psbt)?;

    // Deserialize the PSBT
    let psbt: PartiallySignedTransaction = deserialize(&decoded_psbt)?;

    // Get transaction details
    let tx = psbt.clone().extract_tx();

    // Get the txid
    let txid = tx.txid().to_string();

    // Get the input addresses
    let input_addresses: Vec<String> = psbt
        .inputs
        .iter()
        .enumerate()
        .filter_map(|(index, input)| {
            let prevout = tx.input[index].previous_output;
            input
                .witness_utxo
                .as_ref()
                .or_else(|| input.non_witness_utxo.as_ref()?.output.get(prevout.vout as usize))
                .map(|output| Address::from_script(&output.script_pubkey, network))
        })
        .filter_map(|addr| addr)
        .map(|addr| addr.to_string())
        .collect();

    // Get the send address and total amount
    let send_address;
    let total_amount;
    let output = &tx.output[0];
    if let Some(address) = Address::from_script(&output.script_pubkey, network) {
        send_address = address.to_string();
        total_amount = output.value;
    } else {
        return Err("Invalid output address".into());
    }

    // Calculate the fee
    let input_amount: u64 = psbt
        .inputs
        .iter()
        .enumerate()
        .filter_map(|(index, input)| {
            let prevout = tx.input[index].previous_output;
            input
                .witness_utxo
                .as_ref()
                .or_else(|| input.non_witness_utxo.as_ref()?.output.get(prevout.vout as usize))
                .map(|output| output.value)
        })
        .sum();

    let fee = input_amount - tx.output.iter().map(|output| output.value).sum::<u64>();

    // Create the JSON object
    let result = json!({
        "txid": txid,
        "send_address": send_address,
        "input_addresses": input_addresses,
        "fee": fee,
        "total_amount": total_amount,
    });

    Ok(result)
}

fn main() {
    let base64_psbt = "cHNidP8BAH0BAAAAAdAW0fnwMGYPMqstz4OSdpISfRRFqUUBMG6zTWoS4efaAAAAAAD9////AuJEAAAAAAAAIgAgSvL25OUMP0hwhBoBK4JWzBB2DwM94NhyEsKve8+JbCjoAwAAAAAAABYAFLTI5y7+f3LbbI3D/NpL65mwux5RCwolAAABAP1bAQEAAAAAAQEfO9PBcoAslgtoed83CQqpFvsRdxwstQ10N2HzNdqdHgAAAAAA/v///wKASQAAAAAAACIAILmGho/bMu5shj75Emjq5uR6msL6lc3vlEBqQeJfZ0bg6gMAAAAAAAAWABT8sJkS7IVjbkC/6bFynhFRf+JupQMARzBEAiBjuRSXbEituqaV8ulfcxI62+7svFQ0ZAPqlLfEsvPFwgIgNQEdzuzmE05zCdBWz5D+u2KF8Fncle0fiMjJhv9qVC0BkVEhAvqn8w25vLaWUgEb0/5IuPkEqfEl1R4r5krx/hXU90CJIQOdlP7/99bWcU52WgsWKdi9cl0yFNUwWHUpogbKJ81ImFKuZFEhAmnLtZoMZeOWWyGhn1sMDGJ1UOwl8os2nsKLH0flmzvOIQNBD1APODznI6ZVn3sSwoeHgqxfIxdBoLGCJKxsOOWPDVKvaFGm9yQAAQErgEkAAAAAAAAiACC5hoaP2zLubIY++RJo6ubkeprC+pXN75RAakHiX2dG4AEFkVEhAzC0MbE8y3TOip/KPxlVNmV3Q1xGST2XPDSXTGmu0ZwcIQJ47Jdx6vg2C/c8jRp86Hejq6+8hACL56RM4PRfTJUA5FKuZFEhAn9puKCUw4woSojoEaAF281G5VXWFIZ2JQSwMkAiVYUpIQOrZ73iJizUHCMubR/cfOINi3noMBKDDmDfL0tZ1A+KJFKvaFEiBgJ47Jdx6vg2C/c8jRp86Hejq6+8hACL56RM4PRfTJUA5BiaaiWAVAAAgAEAAIAAAACAAAAAAAEAAAAiBgJ/abiglMOMKEqI6BGgBdvNRuVV1hSGdiUEsDJAIlWFKRhj43UmVAAAgAEAAIAAAACAAAAAAAEAAAAiBgMwtDGxPMt0zoqfyj8ZVTZld0NcRkk9lzw0l0xprtGcHBgStKcNVAAAgAEAAIAAAACAAAAAAAEAAAAiBgOrZ73iJizUHCMubR/cfOINi3noMBKDDmDfL0tZ1A+KJBi9giqnVAAAgAEAAIAAAACAAAAAAAEAAAAAAQGRUSECrrZFU9MUPkV5N+BDet3cNgSmBzzkrhI2ExCVTbwp0n4hAwD0fG8Hcz9uVqGuHfF8FBl9M9FAem1kXJ3sj6OuA88ZUq5kUSEDWSvleVbWe95GxWwzuZ8aH1ltZXCI5pRovfs6v+q2K3MhA6XzA+f8l8ul/Kv0lKNoveZ6AEeu0DYRze9T6cdq+ds2Uq9oUSICAq62RVPTFD5FeTfgQ3rd3DYEpgc85K4SNhMQlU28KdJ+GBK0pw1UAACAAQAAgAAAAIABAAAAAAAAACICAwD0fG8Hcz9uVqGuHfF8FBl9M9FAem1kXJ3sj6OuA88ZGJpqJYBUAACAAQAAgAAAAIABAAAAAAAAACICA1kr5XlW1nveRsVsM7mfGh9ZbWVwiOaUaL37Or/qtitzGGPjdSZUAACAAQAAgAAAAIABAAAAAAAAACICA6XzA+f8l8ul/Kv0lKNoveZ6AEeu0DYRze9T6cdq+ds2GL2CKqdUAACAAQAAgAAAAIABAAAAAAAAAAAA"; // Replace this with a real base64 PSBT

    match parse_psbt(base64_psbt, Some(Network::Testnet)) { // Change the network here
        Ok(parsed) => {
            println!("Parsed PSBT: {}", parsed);
        }
        Err(e) => {
            eprintln!("Error parsing PSBT: {}", e);
        }
    }
}
