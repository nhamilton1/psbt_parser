use axum::http::StatusCode;
use axum::{routing::{post, get}, Router, Server};
use bitcoin::consensus::encode::deserialize;
use bitcoin::util::psbt::PartiallySignedTransaction;
use bitcoin::Address;
use bitcoin::Network;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};

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


async fn parse_psbt_handler(
    axum::Json(ParsePsbtRequest { psbt, network }): axum::Json<ParsePsbtRequest>,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    let result = parse_psbt(&psbt, network).map_err(|_| StatusCode::BAD_REQUEST)?;

    Ok(axum::response::Json(result))
}

#[tokio::main]
async fn main() {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3069));

    let service = router().await.into_make_service();
    Server::bind(&addr)
        .serve(service)
        .await
        .expect("server failed");
}

async fn router() -> Router {
    let cors = CorsLayer::new().allow_origin(Any);

    Router::new()
        .layer(cors)
        .route("/parse_psbt", post(parse_psbt_handler))
        .route("/", get(|| async { "health check" }))
}
