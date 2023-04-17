use bitcoin::consensus::encode::deserialize;
use bitcoin::util::psbt::PartiallySignedTransaction;
use bitcoin::Address;
use bitcoin::Network;
use lambda_http::{run, service_fn, Body, Error, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

fn serialize_network<S>(network: &Option<Network>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match network {
        Some(n) => serializer.serialize_str(&n.to_string()),
        None => serializer.serialize_none(),
    }
}

fn deserialize_network<'de, D>(deserializer: D) -> Result<Option<Network>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    match s {
        Some(s) => {
            Ok(Some(s.parse().map_err(|_| {
                serde::de::Error::custom("failed to parse network")
            })?))
        }
        None => Ok(None),
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ParsePsbtRequest {
    psbt: String,
    #[serde(
        serialize_with = "serialize_network",
        deserialize_with = "deserialize_network"
    )]
    network: Option<Network>,
}

pub fn parse_psbt(
    base64_psbt: &str,
    network: Option<Network>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
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
                .or_else(|| {
                    input
                        .non_witness_utxo
                        .as_ref()?
                        .output
                        .get(prevout.vout as usize)
                })
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
                .or_else(|| {
                    input
                        .non_witness_utxo
                        .as_ref()?
                        .output
                        .get(prevout.vout as usize)
                })
                .map(|output| output.value)
        })
        .sum();

    let fee = input_amount - tx.output.iter().map(|output| output.value).sum::<u64>();

    let mut pay_to_info = Vec::new();
    for output in tx.output {
        let address = Address::from_script(&output.script_pubkey, network).unwrap();
        pay_to_info.push(json!({
            "amount": output.value,
            "pay_to": address.to_string(),
        }));
    }

    let result = json!({
        "txid": txid,
        "send_address": send_address,
        "input_addresses": input_addresses,
        "fee": fee,
        "total_amount": total_amount,
        "pay_to_info": pay_to_info,
    });

    Ok(result)
}

#[derive(Debug, Deserialize)]
struct LambdaRequest {
    psbt: String,
    network: Option<String>,
}

#[derive(Debug, Serialize)]
struct LambdaResponse {
    txid: String,
    send_address: String,
    input_addresses: Vec<String>,
    fee: u64,
    total_amount: u64,
    pay_to_info: Vec<serde_json::Value>,
}

async fn function_handler(event: Request) -> Result<Response<Body>, Error> {
    let body = event.into_body();

    let lambda_request: LambdaRequest = serde_json::from_slice(&body).unwrap();
    
    let network = lambda_request.network.map(|n| n.parse().unwrap_or(Network::Testnet));

    let result = parse_psbt(&lambda_request.psbt, network).unwrap();

    let response = LambdaResponse {
        txid: result["txid"].as_str().unwrap().to_owned(),
        send_address: result["send_address"].as_str().unwrap().to_owned(),
        input_addresses: serde_json::from_value(result["input_addresses"].clone()).unwrap(),
        fee: result["fee"].as_u64().unwrap(),
        total_amount: result["total_amount"].as_u64().unwrap(),
        pay_to_info: serde_json::from_value(result["pay_to_info"].clone()).unwrap(),
    };

    let response_json = serde_json::to_string(&response).unwrap();

    Ok(Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(Body::from(response_json))
        .unwrap())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();
    run(service_fn(function_handler)).await
}
