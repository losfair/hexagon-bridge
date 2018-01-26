use super::provider::{ContextOwner, ContextHandle};
use hexagon_vm_core::hybrid::executor::Executor;
use hexagon_vm_core::hybrid::program_context::{
    ProgramContext,
    CommonProgramContext
};
use super::provider::{InvokeCallback, GenericJitProvider};
use std::ptr;
use hexagon_vm_core::hybrid::program::{Program, ProgramInfo};

#[allow(improper_ctypes)]
extern "C" {
    fn hexagon_hybrid_external_global_invoke_callback(handle: *const ContextHandle, fn_id: u32, user_data: usize) -> i32;
}

#[no_mangle]
pub extern "C" fn hexagon_hybrid_executor_create() -> *mut Executor {
    Box::into_raw(Box::new(Executor::new()))
}

#[no_mangle]
pub unsafe extern "C" fn hexagon_hybrid_executor_destroy(e: *mut Executor) {
    Box::from_raw(e);
}

unsafe extern "C" fn call_global_invoke(handle: *const ContextHandle, fn_id: u32, user_data: usize) -> i32 {
    hexagon_hybrid_external_global_invoke_callback(handle, fn_id, user_data)
}

#[no_mangle]
pub extern "C" fn hexagon_hybrid_executor_load_program<'a>(
    e: &'a Executor,
    code: *const u8,
    len: u32,
    on_fn_invoke: Option<InvokeCallback>,
    user_data: usize
) -> *mut ContextOwner<'a> {
    let on_fn_invoke = if let Some(f) = on_fn_invoke {
        f
    } else {
        call_global_invoke
    };
    let code = unsafe {
        ::std::slice::from_raw_parts(code, len as usize)
    };
    let program_info = match ProgramInfo::std_deserialize(code) {
        Some(v) => v,
        None => return ptr::null_mut()
    };
    let program = match Program::load(program_info, |_| None) {
        Some(v) => v,
        None => return ptr::null_mut()
    };

    let ctx = ProgramContext::new(
        e,
        program,
        Some(GenericJitProvider {
            on_fn_invoke: on_fn_invoke,
            user_data: user_data
        })
    );
    let owner = ContextOwner {
        context: ctx
    };

    Box::into_raw(Box::new(owner))
}

#[no_mangle]
pub unsafe extern "C" fn hexagon_hybrid_context_destroy<'a>(
    ctx: *mut ContextOwner<'a>
) {
    Box::from_raw(ctx);
}

#[no_mangle]
pub unsafe extern "C" fn hexagon_hybrid_context_run(
    ctx: &ContextOwner
) {
    ctx.context.get_executor().eval_program(&ctx.context, 0);
}

#[no_mangle]
pub unsafe extern "C" fn hexagon_hybrid_context_set_global(
    ctx: &ContextOwner,
    id: u32,
    value: u32
) {
    ctx.context.get_executor().write_global(id as usize, value as u64);
}

#[no_mangle]
pub unsafe extern "C" fn hexagon_hybrid_context_get_global(
    ctx: &ContextOwner,
    id: u32
) -> u32 {
    ctx.context.get_executor().read_global(id as usize) as u32
}
