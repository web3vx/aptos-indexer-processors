use bcs::from_bytes;
use regex::Regex;
use serde_json::Value;

use move_core_types::identifier::Identifier;
use move_core_types::language_storage::ModuleId;
use move_core_types::value::MoveValue;

use crate::custom_processor::types::multisig::MultisigTransactionPayload;
use crate::custom_processor::utils::mapper::{map_string_to_move_type, parse_nested_move_values};

pub fn parse_event_data(data: &str) -> anyhow::Result<Value> {
    serde_json::from_str(data).map_err(anyhow::Error::new)
}

pub fn decode_event_payload(event_data: &Value) -> anyhow::Result<Vec<u8>> {
    let payload_str = event_data["transaction"]["payload"]["vec"]
        .as_array()
        .ok_or(anyhow::anyhow!("Payload vector missing"))?
        .get(0)
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Payload string missing"));
    match payload_str {
        Ok(payload_str) => hex::decode(payload_str.strip_prefix("0x").unwrap_or(payload_str))
            .map_err(anyhow::Error::new),
        Err(e) => Err(e),
    }
}

pub fn parse_payload(payload: &[u8]) -> anyhow::Result<MultisigTransactionPayload> {
    from_bytes::<MultisigTransactionPayload>(payload).map_err(anyhow::Error::new)
}

pub async fn process_entry_function(
    payload_parsed: &MultisigTransactionPayload,
) -> anyhow::Result<Value> {
    let MultisigTransactionPayload::EntryFunction(ref entry) = *payload_parsed else {
        return Err(anyhow::anyhow!("Payload is not EntryFunction"));
    };

    let function_details = fetch_function_details(&entry.module).await?;
    let parsed_args = parse_function_args(&function_details, &entry.args, &entry.function)?;
    let mut json_payload = serde_json::to_value(&payload_parsed)?;
    json_payload["EntryFunction"]["parsed_args"] = serde_json::to_value(parsed_args)?;
    Ok(json_payload)
}

async fn fetch_function_details(module: &ModuleId) -> anyhow::Result<Value> {
    let request_url = format!(
        "https://fullnode.mainnet.aptoslabs.com/v1/accounts/{}/module/{}",
        module.address, module.name
    );
    let response = reqwest::get(&request_url).await?;
    let result = response
        .json::<Value>()
        .await
        .map_err(|error| anyhow::anyhow!("Error: {:?}", error));
    if result.is_err() {
        return result;
    }
    // If the module is not found, try fetching from testnet
    let error_regex = Regex::new("module_not_found").unwrap();
    if error_regex.is_match(&result.as_ref().unwrap().to_string()) {
        let request_url = format!(
            "https://fullnode.testnet.aptoslabs.com/v1/accounts/{}/module/{}",
            module.address, module.name
        );
        let response = reqwest::get(&request_url).await?;
        let result = response
            .json::<Value>()
            .await
            .map_err(|error| anyhow::anyhow!("Error: {:?}", error));
        return result;
    }
    result
}

pub fn parse_function_args(
    function_details: &Value,
    args: &Vec<Vec<u8>>,
    function: &Identifier,
) -> anyhow::Result<Vec<Value>> {
    let function_params = function_details["abi"]["exposed_functions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Function details missing"))?
        .into_iter()
        .find(|f| f["name"].as_str().unwrap() == function.as_str())
        .ok_or_else(|| anyhow::anyhow!("Function not found"))?["params"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Parameters missing"))?
        .iter()
        .filter(|&x| x.as_str() != Some("&signer"))
        .collect::<Vec<_>>();
    if args.len() != function_params.len() {
        return Ok(Vec::new());
    };
    args.iter()
        .enumerate()
        .map(|(index, arg)| {
            let type_layout = map_string_to_move_type(function_params[index].as_str().unwrap());
            if type_layout.is_none() {
                return Ok(Value::Null);
            };
            let move_value = MoveValue::simple_deserialize(arg, &type_layout.unwrap())?;
            let json_vec = parse_nested_move_values(&move_value);
            let json_value = serde_json::from_str(&json_vec);
            if json_value.is_err() {
                tracing::warn!("Error parsing JSON: {:?}", json_value.err());
                return Ok(Value::Null);
            }
            Ok(json_value.unwrap())
        })
        .collect()
}
