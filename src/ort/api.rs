use std::os::raw::c_char;
use std::ffi::CStr;
use std::ptr::{null, null_mut};
use std::panic::{AssertUnwindSafe, catch_unwind};
use hexagon_vm_core::executor::{Executor, ExecutorImpl};
use hexagon_vm_core::value::Value;
use hexagon_vm_core::function::Function;
use hexagon_vm_core::function::VirtualFunctionInfo;
use hexagon_vm_core::errors::VMError;

use rmp_serde;
use serde_json;

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
        || e.invoke(target, this, args)
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
pub unsafe extern "C" fn hexagon_ort_function_destroy(
    f: *mut Function
) {
    Box::from_raw(f);
}

#[no_mangle]
pub extern "C" fn hexagon_ort_function_load_native(
    cb: extern "C" fn (*mut Value /* ret_place */, &mut ExecutorImpl, *const ()),
    user_data: *const ()
) -> *mut Function {
    let f = Box::new(move |e: &mut ExecutorImpl| {
        unsafe {
            let mut ret: Value = ::std::mem::zeroed();
            cb(&mut ret, e, user_data);
            ret
        }
    });
    let f = Function::from_native(f);
    Box::into_raw(Box::new(f))
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
                Err(_) => {
                    eprintln!("CFG verification failed");
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
                Err(_) => {
                    eprintln!("CFG verification failed");
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
