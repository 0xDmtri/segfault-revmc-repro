use std::{collections::BTreeSet, sync::Arc};

use alloy::{
    network::AnyNetwork,
    primitives::{keccak256, utils::parse_ether, B256, U256},
    providers::{ProviderBuilder, RootProvider},
    rpc::client::ClientBuilder,
    sol_types::SolValue,
    transports::http::{Client, Http},
};
use eyre::Result;
use foundry_fork_db::{cache::BlockchainDbMeta, BlockchainDb, SharedBackend};
use lazy_static::lazy_static;
use revm::{
    db::CacheDB,
    primitives::{AccountInfo, Bytecode, ExecutionResult, Output, TransactTo},
};

use repro::{
    build_evm_with_libdexy,
    common::{
        CALLDATA, LIB_DEXY_ADDRESS, LIB_DEXY_CODE, LIB_DEXY_CONTROLLER, LIB_DEXY_HASH, WETH_ADDRESS,
    },
};

const ENDPOINT: Option<&str> = option_env!("RPC_URL");
const FORK_BLOCK_NUM: u64 = 20483940_u64;

fn main() -> Result<()> {
    let Some(endpoint) = ENDPOINT else {
        return Ok(());
    };

    let provider = get_http_provider(endpoint);
    let meta = BlockchainDbMeta {
        cfg_env: Default::default(),
        block_env: Default::default(),
        hosts: BTreeSet::from([endpoint.to_string()]),
    };

    let backend = SharedBackend::spawn_backend_thread(
        Arc::new(provider),
        BlockchainDb::new(meta, None),
        Some(FORK_BLOCK_NUM.into()),
    );

    let block = backend.get_full_block(FORK_BLOCK_NUM)?;

    let db = CacheDB::new(backend);

    let mut evm = build_evm_with_libdexy(db, &block);

    {
        spoof_storage(evm.db_mut(), parse_ether("69")?)?;
    }

    let tx = evm.tx_mut();

    tx.caller = LIB_DEXY_CONTROLLER;
    tx.transact_to = TransactTo::Call(LIB_DEXY_ADDRESS);
    tx.data = CALLDATA.clone();
    tx.gas_limit = 700000;
    tx.gas_price = U256::from(block.header.base_fee_per_gas.expect("Base fee on mainnet"));
    tx.value = U256::ZERO;

    let result = match evm.transact_commit() {
        Ok(result) => result,
        Err(e) => return Err(e.into()),
    };

    let output = match result {
        ExecutionResult::Success { output, .. } => match output {
            Output::Call(o) => o,
            Output::Create(o, _) => o,
        },
        ExecutionResult::Revert { output, .. } => return Err(eyre::eyre!("Revert: {:?}", output)),
        ExecutionResult::Halt { reason, .. } => return Err(eyre::eyre!("Halt: {:?}", reason)),
    };

    println!("{:?}", output);

    Ok(())
}

fn get_http_provider(endpoint: &str) -> RootProvider<Http<Client>, AnyNetwork> {
    ProviderBuilder::new()
        .network::<AnyNetwork>()
        .on_client(ClientBuilder::default().http(endpoint.parse().unwrap()))
}

// 32 bytes hash of the storage slot
lazy_static! {
    static ref SLOT: U256 = {
        let encoded = (LIB_DEXY_ADDRESS, U256::from(3)).abi_encode();
        keccak256(encoded).into()
    };
}

// Lil Dexy AccountInfo
lazy_static! {
    static ref LIB_DEXY_INFO: AccountInfo = AccountInfo::new(
        U256::ZERO,
        0,
        LIB_DEXY_HASH,
        Bytecode::new_raw(LIB_DEXY_CODE.clone()),
    );
}

// Controller AccountInfo
lazy_static! {
    static ref CONTROLLER_INFO: AccountInfo = AccountInfo::new(
        parse_ether("420").expect("Ether in Wei"),
        0,
        B256::default(),
        Bytecode::default()
    );
}

/// Inserts custom minimal router contract into evm instance for simulations
#[inline]
fn spoof_storage(db: &mut CacheDB<SharedBackend>, weth_bal: U256) -> Result<()> {
    // insert lilRouter bytecode
    db.insert_account_info(LIB_DEXY_ADDRESS, (*LIB_DEXY_INFO).clone());

    // insert and fund lilDeximus controller (so we can spoof, yikes)
    db.insert_account_info(LIB_DEXY_CONTROLLER, (*CONTROLLER_INFO).clone());

    // fund lilDexy
    db.insert_account_storage(WETH_ADDRESS, *SLOT, weth_bal)?;

    Ok(())
}
