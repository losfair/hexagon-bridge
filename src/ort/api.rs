use std::os::raw::c_char;
use std::ffi::CStr;
use std::ptr::null_mut;
use std::panic::{AssertUnwindSafe, catch_unwind};
use hexagon_vm_core::executor::{Executor, ExecutorImpl};
use hexagon_vm_core::value::Value;
use hexagon_vm_core::function::Function;
use hexagon_vm_core::function::VirtualFunctionInfo;

use rmp_serde;
use serde_json;

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

#[no_mangle]
pub extern "C" fn hexagon_ort_function_load_native(
    cb: extern "C" fn (&mut ExecutorImpl, *const ()) -> *mut Value,
    user_data: *const ()
) -> *mut Function {
    let f = Box::new(move |e: &mut ExecutorImpl| {
        let ret = cb(e, user_data);
        if ret.is_null() {
            Value::Null
        } else {
            *unsafe { Box::from_raw(ret) }
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
