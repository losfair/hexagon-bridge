use hexagon_vm_core::hybrid::jit::JitProvider;
use hexagon_vm_core::hybrid::program_context::{
    ProgramContext,
    CommonProgramContext
};

pub struct ContextOwner<'a> {
    pub(crate) context: ProgramContext<'a, GenericJitProvider>
}

pub struct ContextHandle<'a> {
    pub(crate) _context: &'a CommonProgramContext
}

pub type InvokeCallback = unsafe extern "C" fn (handle: *const ContextHandle, fn_id: u32, user_data: usize) -> i32;
pub struct GenericJitProvider {
    pub(crate) on_fn_invoke: InvokeCallback,
    pub(crate) user_data: usize
}

impl JitProvider for GenericJitProvider {
    fn invoke_function(&self, ctx: &CommonProgramContext, id: usize) -> bool {
        let ctx_handle = ContextHandle {
            _context: ctx
        };
        match unsafe { (self.on_fn_invoke)(&ctx_handle, id as u32, self.user_data) } {
            0 => false,
            _ => true
        }
    }
}
