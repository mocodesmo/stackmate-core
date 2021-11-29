use bdk::blockchain::esplora::{EsploraBlockchain, EsploraBlockchainConfig, EsploraError};
use bdk::blockchain::ConfigurableBlockchain;

use bitcoin::network::constants::Network;

use crate::e::{ErrorKind, S5Error};

pub struct WalletConfig {
  pub deposit_desc: String,
  pub change_desc: String,
  pub network: Network,
  pub client: EsploraBlockchain,
}

pub enum BlockchainBackend {
  Electrum,
  Esplora,
  Rpc,
}

pub const DEFAULT: &str = "default";
pub const DEFAULT_TESTNET_NODE: &str = "ssl://electrum.blockstream.info:60002";
pub const DEFAULT_MAINNET_NODE: &str = "ssl://electrum.blockstream.info:50002";

impl WalletConfig {
  pub fn new(
    deposit_desc: &str,
    backend: BlockchainBackend,
    node_address: &str,
    socks5: Option<String>,
  ) -> Result<Self, S5Error> {
    let change_desc: &str = &deposit_desc.replace("/0/*", "/1/*");
    let network = if <&str>::clone(&deposit_desc).contains("xpub")
      || <&str>::clone(&deposit_desc).contains("xprv")
    {
      Network::Bitcoin
    } else {
      Network::Testnet
    };

    let node_address = if node_address.contains(DEFAULT) {
      match network {
        Network::Bitcoin => DEFAULT_MAINNET_NODE,
        _ => DEFAULT_TESTNET_NODE,
      }
    } else {
      node_address
    };

    let config = if socks5.is_none() {
      EsploraBlockchainConfig {
        base_url: node_address.to_string(),
        proxy: None,
        timeout_read: 5,
        timeout_write: 5,
        stop_gap: 1000,
      }
    } else {
      EsploraBlockchainConfig {
        base_url: node_address.to_string(),
        proxy: socks5,
        timeout_read: 5,
        timeout_write: 5,
        stop_gap: 1000,
      }
    };

    let client = match EsploraBlockchain::from_config(&config) {
      Ok(result) => result,
      Err(bdk_error) => match bdk_error {
        bdk::Error::Esplora(esplora_error) => match *esplora_error {
          EsploraError::Io(c_error) => {
            return Err(S5Error::new(ErrorKind::Network, &c_error.to_string()))
          }
          e_error => return Err(S5Error::new(ErrorKind::Internal, &e_error.to_string())),
        },
        e_error => return Err(S5Error::new(ErrorKind::Internal, &e_error.to_string())),
      },
    };

    Ok(WalletConfig {
      deposit_desc: deposit_desc.to_string(),
      change_desc: change_desc.to_string(),
      network,
      client,
    })
  }
}

// pub fn _check_client(network: Network, node_address: &str) -> Result<bool, S5Error> {
//   let client: AnyBlockchain = if node_address.contains("electrum") {
//     let config = ElectrumBlockchainConfig {
//       url: node_address.to_string(),
//       socks5: None,
//       retry: 1,
//       timeout: Some(5),
//       stop_gap: 1000,
//     };
//     match create_blockchain_client(AnyBlockchainConfig::Electrum(config)) {
//       Ok(client) => client,
//       Err(e) => return Err(S5Error::new(ErrorKind::Internal, &e.message)),
//     }
//   } else if node_address.contains("http") {
//     let parts: Vec<&str> = node_address.split("?auth=").collect();
//     let auth = if parts[1].is_empty() {
//       Auth::None
//     } else {
//       Auth::UserPass {
//         username: parts[1].split(':').collect::<Vec<&str>>()[0].to_string(),
//         password: parts[1].split(':').collect::<Vec<&str>>()[1].to_string(),
//       }
//     };
//     let config = RpcConfig {
//       url: parts[0].to_string(),
//       auth,
//       network,
//       wallet_name: "ping".to_string(),
//       skip_blocks: None,
//     };

//     match create_blockchain_client(AnyBlockchainConfig::Rpc(config)) {
//       Ok(client) => client,
//       Err(e) => return Err(S5Error::new(ErrorKind::Internal, &e.message)),
//     }
//   } else {
//     return Err(S5Error::new(ErrorKind::Internal, "Invalid Node Address."));
//   };

//   match client.estimate_fee(1) {
//     Ok(_) => Ok(true),
//     Err(e) => Err(S5Error::new(ErrorKind::Network, &e.to_string())),
//   }
// }

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::WalletConfig;
  use bdk::blockchain::Blockchain;
  use bitcoin::network::constants::Network;
  
  #[test]
  fn test_config_errors() {
    let dummy_desc = "xprv/0/*";
    let node_address = "ssl://electrum.blockstream.info:5002";
    let config_error =
      WalletConfig::new(&dummy_desc, BlockchainBackend::Esplora, node_address, None)
        .err()
        .unwrap();
    println!("{:#?}", config_error);
  }
}
