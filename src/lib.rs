pub mod common;

// This dependency is needed to define the necessary symbols used by the compiled bytecodes,
// but we don't use it directly, so silence the unused crate depedency warning.
use revmc_builtins as _;

use std::sync::Arc;

use alloy::{
    primitives::{B256, U256},
    rpc::types::Block,
};
use revm::{handler::register::EvmHandler, Database};
use revmc_context::EvmCompilerFn;

use crate::common::{COINBASE, LIB_DEXY_HASH};

revmc_context::extern_revmc! {
    fn libdexy;
}

pub struct ExternalContext;

impl ExternalContext {
    fn new() -> Self {
        Self
    }

    fn get_function(&self, bytecode_hash: B256) -> Option<EvmCompilerFn> {
        if bytecode_hash == LIB_DEXY_HASH {
            return Some(EvmCompilerFn::new(libdexy));
        }

        None
    }
}

#[inline]
fn register_handler<DB: Database + 'static>(handler: &mut EvmHandler<'_, ExternalContext, DB>) {
    let prev = handler.execution.execute_frame.clone();
    handler.execution.execute_frame = Arc::new(move |frame, memory, tables, context| {
        let interpreter = frame.interpreter_mut();
        let bytecode_hash = interpreter.contract.hash.unwrap_or_default();
        if let Some(f) = context.external.get_function(bytecode_hash) {
            Ok(unsafe { f.call_with_interpreter_and_memory(interpreter, memory, context) })
        } else {
            prev(frame, memory, tables, context)
        }
    });
}

#[inline]
pub fn build_evm_with_libdexy<'a, DB: Database + 'static>(
    db: DB,
    block: &Block,
) -> revm::Evm<'a, ExternalContext, DB> {
    revm::Evm::builder()
        .with_db(db)
        .modify_block_env(|revm_block| {
            revm_block.number = U256::from(block.header.number.expect("Block number"));
            revm_block.timestamp = U256::from(block.header.timestamp);
            revm_block.basefee =
                U256::from(block.header.base_fee_per_gas.expect("Base fee on mainnet"));
            revm_block.coinbase = COINBASE;
        })
        .with_external_context(ExternalContext::new())
        .append_handler_register(register_handler)
        .build()
}
