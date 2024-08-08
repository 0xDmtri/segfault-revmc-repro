use revmc::{primitives::SpecId, EvmCompiler, EvmLlvmBackend, OptimizationLevel, Result};
use std::path::PathBuf;

include!("./src/common.rs");

fn main() -> Result<()> {
    // Emit the configuration to run compiled bytecodes
    // This not used if we are only using statically linked bytecodes
    revmc_build::emit();

    // Compile and statically link a bytecode
    let name = "libdexy";

    // Configure revmc
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    let context = revmc::llvm::inkwell::context::Context::create();
    let backend = EvmLlvmBackend::new(&context, true, OptimizationLevel::Aggressive)?;

    // Set compiler for maximum performance
    let mut compiler = EvmCompiler::new(backend);
    compiler.gas_metering(false);
    unsafe { compiler.stack_bound_checks(false) }

    // Compile libdexy and write it to file
    compiler.translate(name, &LIB_DEXY_CODE, SpecId::CANCUN)?;
    let object = out_dir.join(name).with_extension("o");
    compiler.write_object_to_file(&object)?;

    cc::Build::new()
        .object(&object)
        .static_flag(true)
        .compile(name);

    Ok(())
}
