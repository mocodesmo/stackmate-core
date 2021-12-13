use std::ffi::CString;
use std::os::raw::c_char;
use std::str::FromStr;
use std::collections::btree_map::BTreeMap;

use serde::{Deserialize, Serialize};

use bdk::blockchain::noop_progress;
use bdk::database::MemoryDatabase;

use bdk::{SignOptions, Wallet, KeychainKind};

use bitcoin::base64;
use bitcoin::blockdata::transaction::Transaction;
use bitcoin::consensus::deserialize;
use bitcoin::network::constants::Network;
use bitcoin::util::address::Address;
use bitcoin::util::psbt::PartiallySignedTransaction;
use bdk::descriptor::{Descriptor};
use bdk::miniscript::DescriptorTrait;

use crate::config::WalletConfig;
use crate::e::{ErrorKind, S5Error};

use crate::wallet::policy::{SpendingPolicyPaths};

/// FFI Output
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WalletPSBT {
  pub psbt: String,
  pub is_finalized: bool,
}

impl WalletPSBT {
  pub fn c_stringify(&self) -> *mut c_char {
    let stringified = match serde_json::to_string(self) {
      Ok(result) => result,
      Err(_) => {
        return CString::new("Error:JSON Stringify Failed. BAD NEWS! Contact Support.")
          .unwrap()
          .into_raw()
      }
    };

    CString::new(stringified).unwrap().into_raw()
  }
}


pub async fn build(
  config: WalletConfig,
  to: &str,
  amount: Option<u64>,
  fee_absolute: u64,
  sweep: bool,
  policy_paths: Option<SpendingPolicyPaths>
) -> Result<WalletPSBT, S5Error> {
  let wallet = match Wallet::new(
    &config.deposit_desc,
    Some(&config.change_desc),
    config.network,
    MemoryDatabase::default(),
    config.client,
  ).await {
    Ok(result) => result,
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "Wallet-Initialization")),
  };

  match wallet.sync(noop_progress(), None).await {
    Ok(_) => (),
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "Wallet-Sync")),
  };

  let send_to = match Address::from_str(to) {
    Ok(result) => result,
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "Address-Parse")),
  };

  let (psbt, _) = {
    let mut builder = wallet.build_tx();
    if sweep && amount.is_none() {
      builder.drain_wallet().drain_to(send_to.script_pubkey());
    } else {
      builder
        .enable_rbf()
        .add_recipient(send_to.script_pubkey(), amount.unwrap());
    }

    builder.fee_absolute(fee_absolute);

    if policy_paths.is_some(){
      builder.policy_path(policy_paths.clone().unwrap().external, KeychainKind::External);
      builder.policy_path(policy_paths.unwrap().internal, KeychainKind::Internal);
    }

    match builder.finish() {
      Ok(result) => result,
      Err(e) => {
        println!("{:?}", e);
        return Err(S5Error::new(ErrorKind::Internal, &e.to_string()));
      }
    }
  };

  Ok(WalletPSBT {
    psbt: psbt.to_string(),
    is_finalized: false,
  })
}

#[derive(Serialize, Debug, Clone)]
pub struct DecodedTxIO {
  value: u64,
  to: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct DecodedTx {
  pub outputs: Vec<DecodedTxIO>,
  // pub weight: usize,
  // pub satisfaction_weight: usize
}

impl DecodedTx {
  pub fn c_stringify(&self) -> *mut c_char {
    let stringified = match serde_json::to_string(self) {
      Ok(result) => result,
      Err(_) => {
        return CString::new("Error:JSON Stringify Failed. BAD NEWS! Contact Support.")
          .unwrap()
          .into_raw()
      }
    };

    CString::new(stringified).unwrap().into_raw()
  }
}

pub fn decode(network: Network, psbt: &str) -> Result<DecodedTx, S5Error> {
  let decoded_psbt = match base64::decode(psbt) {
    Ok(psbt) => psbt,
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "Basae64-Decode")),
  };

  let psbt_struct: PartiallySignedTransaction = match deserialize(&decoded_psbt) {
    Ok(psbt) => psbt,
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "Deserialize-Error")),
  };

  let outputs = &psbt_struct.global.unsigned_tx.output;
  // println!("{:#?}", Address::from_script(&outputs[0].clone().script_pubkey,network_enum));
  let inputs = &psbt_struct.inputs;

  // let transaction: Transaction = psbt_struct.clone().extract_tx();

  let mut decoded_outputs: Vec<DecodedTxIO> = vec![];

  let mut total_out_value = 0;
  let mut total_in_value = 0;

  for output in outputs {
    total_out_value += output.value;
    decoded_outputs.push(DecodedTxIO {
      value: output.value,
      to: match Address::from_script(&output.script_pubkey, network) {
        Some(address) => address.to_string(),
        None => "None".to_string(),
      },
    });
  }

  for input in inputs {
    // let witness_utxo = input.witness_utxo.clone();
    total_in_value += input.witness_utxo.clone().unwrap().value;
    // decoded_inputs.push(DecodedTxIO {
    //   value: input.witness_utxo.clone().unwrap().value,
    //   to: match Address::from_script(&input.witness_script.clone().unwrap(), network) {
    //     Some(address) => address.to_string(),
    //     None => "None".to_string(),
    //   },
    // });
  }

  decoded_outputs.push(DecodedTxIO {
    value: total_in_value - total_out_value,
    to: "miner".to_string(),
  });

  Ok(DecodedTx {
    outputs: decoded_outputs,
    // weight: weight + outputs.len() * 76,
  })
}


/// FFI Output
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransactionWeight {
  pub weight: usize,
}

impl TransactionWeight {
  pub fn c_stringify(&self) -> *mut c_char {
    let stringified = match serde_json::to_string(self) {
      Ok(result) => result,
      Err(_) => {
        return CString::new("Error:JSON Stringify Failed. BAD NEWS! Contact Support.")
          .unwrap()
          .into_raw()
      }
    };

    CString::new(stringified).unwrap().into_raw()
  }
}

pub fn get_weight(
  deposit_desc: &str,
  psbt: &str
)->Result<TransactionWeight, S5Error>{
  let decoded_psbt = match base64::decode(psbt) {
    Ok(psbt) => psbt,
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "Base64-Decode")),
  };

  let psbt_struct: PartiallySignedTransaction = match deserialize(&decoded_psbt) {
    Ok(psbt) => psbt,
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "Deserialize-Error")),
  };

  let transaction: Transaction = psbt_struct.extract_tx();
  let desc = Descriptor::<String>::from_str(deposit_desc).unwrap();
  let satisfaction_weight = desc.max_satisfaction_weight().unwrap();
  
  Ok(TransactionWeight{
    weight: transaction.get_weight() + satisfaction_weight
  })
 
}

pub fn sign(config: WalletConfig, psbt: &str) -> Result<WalletPSBT, S5Error> {
  let wallet = match Wallet::new_offline(
    &config.deposit_desc,
    Some(&config.change_desc),
    config.network,
    MemoryDatabase::default(),
  ) {
    Ok(result) => result,
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "Wallet-Initialization")),
  };

  let mut final_psbt = match deserialize(&base64::decode(psbt).unwrap()) {
    Ok(psbt) => psbt,
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "Deserialize-Psbt-Error")),
  };

  let finalized = match wallet.sign(&mut final_psbt, SignOptions::default()) {
    Ok(result) => result,
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "Sign-Error")),
  };

  Ok(WalletPSBT {
    psbt: final_psbt.to_string(),
    is_finalized: finalized,
  })
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Txid {
  pub txid: String,
}
impl Txid {
  pub fn c_stringify(&self) -> *mut c_char {
    let stringified = match serde_json::to_string(self) {
      Ok(result) => result,
      Err(_) => {
        return CString::new("Error:JSON Stringify Failed. BAD NEWS! Contact Support.")
          .unwrap()
          .into_raw()
      }
    };

    CString::new(stringified).unwrap().into_raw()
  }
}

pub async fn broadcast(config: WalletConfig, psbt: &str) -> Result<Txid, S5Error> {
  let wallet = match Wallet::new(
    &config.deposit_desc,
    Some(&config.change_desc),
    config.network,
    MemoryDatabase::default(),
    config.client,
  ).await {
    Ok(result) => result,
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "Wallet-Initialization")),
  };

  match wallet.sync(noop_progress(), None).await {
    Ok(_) => (),
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "Wallet-Sync")),
  };

  let decoded_psbt = match base64::decode(&psbt) {
    Ok(result) => result,
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "PSBT-Decode")),
  };
  let psbt_struct: PartiallySignedTransaction = match deserialize(&decoded_psbt) {
    Ok(result) => result,
    Err(_) => return Err(S5Error::new(ErrorKind::Internal, "PSBT-Deserialize")),
  };
  let tx = psbt_struct.extract_tx();
  let txid = match wallet.broadcast(&tx).await {
    Ok(result) => result,
    Err(e) => return Err(S5Error::new(ErrorKind::Internal, &e.to_string())),
  };

  Ok(Txid {
    txid: txid.to_string(),
  })
}

// #[cfg(test)]
// mod tests {
//   use super::*;
//   use crate::config::WalletConfig;
//   use crate::config::{DEFAULT_TESTNET_NODE,BlockchainBackend};
//   use bitcoin::network::constants::Network;
//   use crate::wallet::policy::{raft_policy_paths};
//   use crate::wallet::address;

//   #[test]
//   fn test_send() {
//     let xkey = "[db7d25b5/84'/1'/6']tpubDCCh4SuT3pSAQ1qAN86qKEzsLoBeiugoGGQeibmieRUKv8z6fCTTmEXsb9yeueBkUWjGVzJr91bCzeCNShorbBqjZV4WRGjz3CrJsCboXUe";
//     let deposit_desc = format!("wpkh({}/0/*)", xkey);
//     let node_address = "ssl://electrum.blockstream.info:60002";

//     let config = WalletConfig::new(&deposit_desc, BlockchainBackend::Electrum, node_address, None).unwrap();
//     let xkey = "[db7d25b5/84'/1'/6']tprv8fWev2sCuSkVWYoNUUSEuqLkmmfiZaVtgxosS5jRE9fw5ejL2odsajv1QyiLrPri3ppgyta6dsFaoDVCF4ZdEAR6qqY4tnaosujsPzLxB49";
//     let deposit_desc = format!("wpkh({}/0/*)", xkey);

//     let sign_config = WalletConfig::new(&deposit_desc, BlockchainBackend::Electrum, node_address, None).unwrap();
//     let to = "mkHS9ne12qx9pS9VojpwU5xtRd4T7X7ZUt";
//     let amount = 5_000;
//     let fee_absolute = 420;

//     let psbt_origin = build(config, to, Some(amount), fee_absolute, false, None);
//     let decoded = decode(Network::Testnet, &psbt_origin.clone().unwrap().psbt);
//     println!("Decoded: {:#?}", decoded.clone().unwrap());
//     // assert_eq!(decoded.unwrap()[0].value, amount);
//     let signed = sign(sign_config, &psbt_origin.clone().unwrap().psbt);
//     println!("{:#?}", signed.clone().unwrap());
//     assert_eq!(signed.clone().unwrap().is_finalized, true);
//     // let broadcasted = broadcast(config, &signed.unwrap().psbt);
//     println!("{:#?}",psbt_origin.clone().unwrap());
//     // assert_eq!(broadcasted.clone().unwrap().txid.len(), 64);
//   }

//   #[test]

//   fn test_get_weight(){
//     let xkey = "[db7d25b5/84'/1'/6']tpubDCCh4SuT3pSAQ1qAN86qKEzsLoBeiugoGGQeibmieRUKv8z6fCTTmEXsb9yeueBkUWjGVzJr91bCzeCNShorbBqjZV4WRGjz3CrJsCboXUe";
//     let deposit_desc = format!("wpkh({}/0/*)", xkey);
//     let psbt = "cHNidP8BAHQBAAAAAf3cLERUN9+6X5+1yk3x9XzSCq1417WtB+gB5qNyj+xpAAAAAAD9////AnRxAQAAAAAAFgAUVyorkNVSCsiE4/7OspP52IwquzqIEwAAAAAAABl2qRQ0Sg9IyhUOwrkDgXZgubaLE6ZwJoisAAAAAAABAN4CAAAAAAEByvn9X3PvFqemGsrTv8ivAO07IOeRhBz7J0huqXJLfVgBAAAAAP7///8CoIYBAAAAAAAWABQTXAMs/1Qr5n6pDVK9O15ODZ/UCVZWjQAAAAAAFgAUIixaISTPlO8fwyT3hCL+An5+Km4CRzBEAiBFsQJfBur3eQgO5Vw+EvEgr2CagcVGXw9oYw3FOaMSSgIgch0CV+W3oRCKNBwxqiqIK0C5b1TsGk32HvNM+4Z7IksBIQNP/rsBHKbA98977TzmriFrOuO8hQjNg4ON3goI9/Uwjp0BIAABAR+ghgEAAAAAABYAFBNcAyz/VCvmfqkNUr07Xk4Nn9QJIgYD9WhlKKSeNh6567KTmyKrlitDWZOz/+mms7emVsWjGTsY230ltVQAAIABAACABgAAgAAAAAABAAAAACICAgHPrE7CShQkK90ApPF8xdr+8o7T/sHggOlZNOHIUft/GNt9JbVUAACAAQAAgAYAAIABAAAAAQAAAAAA";
//     let expected_weight = 576;
//     let tx_weight = get_weight(&deposit_desc, &psbt).unwrap();
//     assert_eq!(tx_weight.weight, expected_weight);

//   }

//   #[test] #[ignore]
//   fn test_raft_send(){

//     let desc_primary = "wsh(thresh(1,pk([db7d25b5/84'/1'/6']tprv8fWev2sCuSkVWYoNUUSEuqLkmmfiZaVtgxosS5jRE9fw5ejL2odsajv1QyiLrPri3ppgyta6dsFaoDVCF4ZdEAR6qqY4tnaosujsPzLxB49/0/*),snj:and_v(v:pk([66a0c105/84'/1'/5']tpubDCKvnVh6U56wTSUEJGamQzdb3ByAc6gTPbjxXQqts5Bf1dBMopknipUUSmAV3UuihKPTddruSZCiqhyiYyhFWhz62SAGuC3PYmtAafUuG6R/0/*),after(2105103))))";
//     let desc_secondary = "wsh(thresh(1,pk([db7d25b5/84'/1'/6']tpubDCCh4SuT3pSAQ1qAN86qKEzsLoBeiugoGGQeibmieRUKv8z6fCTTmEXsb9yeueBkUWjGVzJr91bCzeCNShorbBqjZV4WRGjz3CrJsCboXUe/0/*),snj:and_v(v:pk([66a0c105/84'/1'/5']tprv8fdte5erKhRGZySSQcvB1ayUUATESmVYpJ9BEtobSoPGB8vbBRwCYKrcGcmKaRqTp1hdpprDpwVq4Fd7p7VacgwdMywv1Lmet6ZtYHV3uc1/0/*),after(2105103))))";

//     let to = "mkHS9ne12qx9pS9VojpwU5xtRd4T7X7ZUt";
//     let amount = 2_100;
//     let fee_absolute = 1_500;

//     // TOP ME UP
//     // WalletAddress {
//     //   address: "tb1q50x7h63d7fl68s7jrqgkk9jwzmcegga3xqyufv0vugeln423veeqn8e3r6",
//     // }
   
//     // Primary Withdrawl
   
//     let config = WalletConfig::new(&desc_primary, BlockchainBackend::Electrum, DEFAULT_TESTNET_NODE, None).unwrap();
//     let policy_paths = raft_policy_paths(config).unwrap();
    
//     println!("{:#?}", policy_paths);

//     let config = WalletConfig::new(&desc_primary, BlockchainBackend::Electrum, DEFAULT_TESTNET_NODE, None).unwrap();
//     let psbt_origin = build(config, to, Some(amount), fee_absolute, true,Some(policy_paths.primary));

//     let decoded = decode(Network::Testnet, &psbt_origin.clone().unwrap().psbt);
//     println!("Decoded: {:#?}", decoded.clone().unwrap());

//     let config = WalletConfig::new(&desc_primary, BlockchainBackend::Electrum, DEFAULT_TESTNET_NODE, None).unwrap();
//     let signed = sign(config, &psbt_origin.clone().unwrap().psbt);

//     assert_eq!(signed.clone().unwrap().is_finalized, true);

//     let config = WalletConfig::new(&desc_primary, BlockchainBackend::Electrum, DEFAULT_TESTNET_NODE, None).unwrap();
//     let broadcasted = broadcast(config, &signed.clone().unwrap().psbt);
//     println!("{:#?}", broadcasted.clone().unwrap());


//     // Secondary Withdrawal
   
//     let config = WalletConfig::new(&desc_secondary, BlockchainBackend::Electrum, DEFAULT_TESTNET_NODE, None).unwrap();
//     let policy_paths = raft_policy_paths(config).unwrap();
    
//     println!("{:#?}", policy_paths);

//     let config = WalletConfig::new(&desc_secondary, BlockchainBackend::Electrum, DEFAULT_TESTNET_NODE, None).unwrap();
//     let psbt_origin = build(config, to, Some(amount), fee_absolute, true,Some(policy_paths.secondary));

//     let decoded = decode(Network::Testnet, &psbt_origin.clone().unwrap().psbt);
//     println!("Decoded: {:#?}", decoded.clone().unwrap());

//     let config = WalletConfig::new(&desc_secondary, BlockchainBackend::Electrum, DEFAULT_TESTNET_NODE, None).unwrap();
//     let signed = sign(config, &psbt_origin.clone().unwrap().psbt);

//     assert_eq!(signed.clone().unwrap().is_finalized, true);

//     let config = WalletConfig::new(&desc_secondary, BlockchainBackend::Electrum, DEFAULT_TESTNET_NODE, None).unwrap();
//     let broadcasted = broadcast(config, &signed.clone().unwrap().psbt);
//     println!("{:#?}", broadcasted.clone().unwrap());

//   }


// }
