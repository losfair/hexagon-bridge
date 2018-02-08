use std::os::raw::c_char;
use std::ffi::{CStr, CString};
use std::ptr::{null, null_mut};
use std::panic::{AssertUnwindSafe, catch_unwind};
use hexagon_vm_core::executor::{Executor, ExecutorImpl};
use hexagon_vm_core::value::{Value, ValueContext};
use hexagon_vm_core::object_info::ObjectHandle;
use hexagon_vm_core::function::Function;
use hexagon_vm_core::function::VirtualFunctionInfo;
use hexagon_vm_core::errors::VMError;
use super::object_proxy;
use super::object_proxy::ObjectProxy;

use rmp_serde;
use serde_json;

#[no_mangle]
pub unsafe extern "C" fn hexagon_enable_debug() {
    ::hexagon_vm_core::debug::enable();
}

#[no_mangle]
pub extern "C" fn hexagon_ort_get_value_size() -> u32 {
    ::std::mem::size_of::<Value>() as u32
}

#[no_mangle]
pub extern "C" fn hexagon_ort_executor_create() -> *mut Executor {
    Box::into_raw(Box::new(Executor::new()))
}

#[no_mangle]
pub unsafe extern "C" fn hexagon_ort_executor_destroy(e: *mut Executor) {
    Box::from_raw(e);
}

#[no_mangle]
pub extern "C" fn hexagon_ort_executor_get_impl(e: &mut Executor) -> *mut ExecutorImpl {
    &mut *e.handle_mut() as *mut ExecutorImpl
}

#[no_mangle]
pub extern "C" fn hexagon_ort_executor_impl_attach_function(
    e: &mut ExecutorImpl,
    key: *const c_char,
    f: *mut Function
) -> u32 {
    let key = unsafe { CStr::from_ptr(key).to_str().unwrap() };
    let f = unsafe { Box::from_raw(f) };

    match catch_unwind(AssertUnwindSafe(|| e.create_static_object(key, f))) {
        Ok(_) => 0,
        Err(_) => 1
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_executor_impl_run_callable(
    e: &mut ExecutorImpl,
    key: *const c_char
) -> u32 {
    let key = unsafe { CStr::from_ptr(key).to_str().unwrap() };
    match catch_unwind(AssertUnwindSafe(|| e.run_callable(key))) {
        Ok(_) => 0,
        Err(_) => 1
    }
}

// this is actually unsafe but since we do not
// make this pub it is fine
fn write_place<T>(place: *mut T, value: T) {
    unsafe {
        ::std::ptr::write(place, value);
    }
}

/// Returns a reference to the requested static object, otherwise null.
///
/// It should be noted that the address of the same `Value` is **not**
/// guaranteed to be consistent and any attempts to mutate the state
/// of the executor may result in undefined behavior.
#[no_mangle]
pub extern "C" fn hexagon_ort_executor_impl_get_static_object(
    ret_place: *mut Value,
    e: &ExecutorImpl,
    key: *const c_char,
) {
    let key = unsafe { CStr::from_ptr(key).to_str().unwrap() };
    let obj = match e.get_static_object(key) {
        Some(v) => v,
        None => return write_place(ret_place, Value::Null)
    };
    write_place(ret_place, (*obj).into())
}

#[no_mangle]
pub extern "C" fn hexagon_ort_executor_impl_invoke(
    ret_place: *mut Value,
    e: &mut ExecutorImpl,
    target: *const Value,
    this: *const Value,
    args: *const Value,
    n_args: u32
) {
    let target = if target.is_null() {
        Value::Null
    } else {
        unsafe { *target }
    };
    let this = if this.is_null() {
        Value::Null
    } else {
        unsafe { *this }
    };
    let args: &[Value] = unsafe {
        ::std::slice::from_raw_parts(args, n_args as usize)
    };

    let result = catch_unwind(AssertUnwindSafe(
        || e.invoke(target, this, None, args)
    ));
    write_place(ret_place, match result {
        Ok(_) => e.get_current_frame().pop_exec(),
        Err(e) => {
            if let Ok(e) = e.downcast::<VMError>() {
                eprintln!("Invoke failed: {}", e.unwrap().to_string());
            } else {
                eprintln!("Unknown error");
            }
            Value::Null
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn hexagon_ort_executor_impl_set_stack_limit(
    e: &mut ExecutorImpl,
    limit: u32
) {
    e.set_stack_limit(limit as usize);
}

#[no_mangle]
pub extern "C" fn hexagon_ort_executor_impl_get_argument(
    ret_place: *mut Value,
    e: &ExecutorImpl,
    id: u32
) -> i32 {
    match e.get_current_frame().get_argument(id as usize) {
        Some(v) => {
            write_place(ret_place, v);
            0
        },
        None => 1
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_executor_impl_get_n_arguments(
    e: &ExecutorImpl
) -> u32 {
    e.get_current_frame().get_n_arguments() as u32
}

#[no_mangle]
pub unsafe extern "C" fn hexagon_ort_function_destroy(
    f: *mut Function
) {
    Box::from_raw(f);
}

struct NativeFunctionGuard {
    destructor: Option<extern "C" fn (*const ())>,
    user_data: *const (),
    always_false: bool
}

impl Drop for NativeFunctionGuard {
    fn drop(&mut self) {
        if let Some(dtor) = self.destructor {
            (dtor)(self.user_data);
        }
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_function_load_native(
    cb: extern "C" fn (*mut Value /* ret_place */, &mut ExecutorImpl, *const ()) -> i32,
    destructor: Option<extern "C" fn (*const ())>,
    user_data: *const ()
) -> *mut Function {
    let guard = NativeFunctionGuard {
        destructor: destructor,
        user_data: user_data,
        always_false: false
    };

    let f = Box::new(move |e: &mut ExecutorImpl| {
        let _v = guard.always_false;

        unsafe {
            let mut ret: Value = ::std::mem::zeroed();
            let err = cb(&mut ret, e, user_data);

            if err != 0 {
                panic!(VMError::from("Native function returns error"));
            }

            ret
        }
    });
    let f = Function::from_native(f);
    Box::into_raw(Box::new(f))
}

#[no_mangle]
pub extern "C" fn hexagon_ort_function_enable_optimization(
    f: &mut Function
) {
    f.enable_optimization();
}

#[no_mangle]
pub extern "C" fn hexagon_ort_function_load_virtual(
    encoding: *const c_char,
    code: *const u8,
    len: u32
) -> *mut Function {
    let encoding = unsafe { CStr::from_ptr(encoding).to_str().unwrap() };
    let code = unsafe { ::std::slice::from_raw_parts(code ,len as usize) };

    match encoding {
        "json" => {
            let code = match ::std::str::from_utf8(code) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("UTF-8 decoding failed: {}", e);
                    return null_mut();
                }
            };
            let vinfo: VirtualFunctionInfo = match serde_json::from_str(code) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("JSON decoding failed: {}", e);
                    return null_mut();
                }
            };
            let f = match catch_unwind(|| Function::from_virtual_info(vinfo)) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("CFG verification failed: {}", match e.downcast::<VMError>() {
                        Ok(v) => v.unwrap().to_string(),
                        Err(_) => "Unknown error".to_string()
                    });
                    return null_mut();
                }
            };
            Box::into_raw(Box::new(f))
        },
        "msgpack" | "messagepack" => {
            let vinfo: VirtualFunctionInfo = match rmp_serde::decode::from_slice(code) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("MessagePack decoding failed: {}", e);
                    return null_mut();
                }
            };
            let f = match catch_unwind(|| Function::from_virtual_info(vinfo)) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("CFG verification failed: {}", match e.downcast::<VMError>() {
                        Ok(v) => v.unwrap().to_string(),
                        Err(_) => "Unknown error".to_string()
                    });
                    return null_mut();
                }
            };
            Box::into_raw(Box::new(f))
        },
        _ => {
            eprintln!("Unsupported encoding: {}", encoding);
            null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_function_dump_json(
    f: &Function
) -> *mut c_char {
    if let Some(v) = f.to_virtual_info() {
        if let Ok(v) = serde_json::to_string(&v) {
            CString::new(v).unwrap().into_raw()
        } else {
            null_mut()
        }
    } else {
        null_mut()
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_function_debug_print(
    f: &Function
) {
    if let Some(v) = f.to_virtual_info() {
        eprintln!("{:?}", v);
    } else {
        eprintln!("(not printable)");
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_function_bind_this(
    f: &Function,
    v: &Value
) -> i32 {
    if let Err(_) = catch_unwind(AssertUnwindSafe(|| f.bind_this(*v))) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_value_create_from_null(ret_place: *mut Value) {
    write_place(ret_place, Value::Null)
}

#[no_mangle]
pub extern "C" fn hexagon_ort_value_create_from_bool(ret_place: *mut Value, v: u32) {
    write_place(ret_place, Value::Bool(if v == 0 { false } else { true }))
}

#[no_mangle]
pub extern "C" fn hexagon_ort_value_create_from_i64(ret_place: *mut Value, v: i64) {
    write_place(ret_place, Value::Int(v))
}

#[no_mangle]
pub extern "C" fn hexagon_ort_value_create_from_f64(ret_place: *mut Value, v: f64) {
    write_place(ret_place, Value::Float(v))
}

#[no_mangle]
pub extern "C" fn hexagon_ort_value_create_from_string(ret_place: *mut Value, v: *const c_char, e: &mut ExecutorImpl) {
    let id = e.get_object_pool_mut().allocate(Box::new(unsafe { CStr::from_ptr(v).to_str().unwrap() }.to_string()));
    write_place(ret_place, Value::Object(id))
}

#[no_mangle]
pub extern "C" fn hexagon_ort_value_read_i64(ret_place: *mut i64, v: &Value) -> i32 {
    match *v {
        Value::Int(v) => {
            write_place(ret_place, v);
            0
        },
        _ => 1
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_value_read_f64(ret_place: *mut f64, v: &Value) -> i32 {
    match *v {
        Value::Float(v) => {
            write_place(ret_place, v);
            0
        },
        _ => 1
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_value_read_null(v: &Value) -> i32 {
    match *v {
        Value::Null => 0,
        _ => 1
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_value_read_bool(ret_place: *mut i32, v: &Value) -> i32 {
    match *v {
        Value::Bool(v) => {
            write_place(ret_place, if v { 1 } else { 0 });
            0
        },
        _ => 1
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_value_get_type(v: &Value) -> u8 {
    match *v {
        Value::Bool(_) => b'B',
        Value::Float(_) => b'F',
        Value::Int(_) => b'I',
        Value::Null => b'N',
        Value::Object(_) => b'O'
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_value_read_string(v: &Value, executor: &ExecutorImpl) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| CString::new(ValueContext::new(
        v,
        &executor.get_object_pool()
    ).to_str().as_ref()).unwrap().into_raw())).unwrap_or(null_mut())
}

#[no_mangle]
pub extern "C" fn hexagon_ort_value_is_string(v: &Value, executor: &ExecutorImpl) -> u32 {
    if !v.is_object() {
        return 0;
    }

    let ctx = ValueContext::new(v, executor.get_object_pool());
    match ctx.as_object_direct().as_any().downcast_ref::<String>() {
        Some(_) => 1,
        None => 0
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_value_to_object_handle<'a>(v: &Value, executor: &'a ExecutorImpl) -> *mut ObjectHandle<'a> {
    if let Value::Object(id) = *v {
        Box::into_raw(Box::new(executor.get_object_pool().get(id)))
    } else {
        null_mut()
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_object_handle_to_object_proxy(handle: &ObjectHandle) -> *const ObjectProxy {
    match handle.as_any().downcast_ref::<ObjectProxy>() {
        Some(v) => v,
        None => null()
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_object_handle_to_function(handle: &ObjectHandle) -> *const Function {
    match handle.as_any().downcast_ref::<Function>() {
        Some(v) => v,
        None => null()
    }
}

#[no_mangle]
pub unsafe extern "C" fn hexagon_ort_object_handle_destroy(h: *mut ObjectHandle) {
    Box::from_raw(h);
}

#[no_mangle]
pub extern "C" fn hexagon_ort_executor_pin_object_proxy(
    ret_place: *mut Value,
    e: &mut ExecutorImpl,
    p: *mut ObjectProxy
) {
    let p = unsafe {
        Box::from_raw(p)
    };
    let id = e.get_object_pool_mut().allocate(p);
    write_place(ret_place, Value::Object(id))
}

#[no_mangle]
pub extern "C" fn hexagon_ort_executor_pin_function(
    ret_place: *mut Value,
    e: &mut ExecutorImpl,
    f: *mut Function
) {
    let f = unsafe {
        Box::from_raw(f)
    };
    let id = e.get_object_pool_mut().allocate(f);
    write_place(ret_place, Value::Object(id))
}

#[no_mangle]
pub extern "C" fn hexagon_ort_object_proxy_create(data: *const ()) -> *mut ObjectProxy {
    Box::into_raw(Box::new(ObjectProxy::new(data)))
}

#[no_mangle]
pub extern "C" fn hexagon_ort_object_proxy_get_data(
    p: &ObjectProxy
) -> *const () {
    p.data
}

#[no_mangle]
pub unsafe extern "C" fn hexagon_ort_object_proxy_destroy(
    p: *mut ObjectProxy
) {
    Box::from_raw(p);
}

#[no_mangle]
pub extern "C" fn hexagon_ort_object_proxy_freeze(
    p: &mut ObjectProxy
) {
    p.frozen = true;
}

#[no_mangle]
pub extern "C" fn hexagon_ort_object_proxy_add_const_field(
    p: &mut ObjectProxy,
    name: *const c_char
) {
    let name = unsafe { CStr::from_ptr(name).to_str().unwrap() }.to_string();
    p.const_fields.insert(name);
}

#[no_mangle]
pub extern "C" fn hexagon_ort_object_proxy_set_destructor(
    p: &mut ObjectProxy,
    f: Option<object_proxy::Destructor>
) {
    p.destructor = f;
}

#[no_mangle]
pub extern "C" fn hexagon_ort_object_proxy_set_static_field(
    p: &mut ObjectProxy,
    k: *const c_char,
    v: *const Value
) {
    let k = unsafe { CStr::from_ptr(k).to_str().unwrap() };
    if v.is_null() {
        p.static_fields.remove(k);
    } else {
        p.static_fields.insert(k.to_string(), unsafe { *v });
    }
}

#[no_mangle]
pub extern "C" fn hexagon_ort_object_proxy_set_on_call(
    p: &mut ObjectProxy,
    f: Option<object_proxy::OnCall>
) {
    p.on_call = f;
}

#[no_mangle]
pub extern "C" fn hexagon_ort_object_proxy_set_on_get_field(
    p: &mut ObjectProxy,
    f: Option<object_proxy::OnGetField>
) {
    p.on_get_field = f;
}
