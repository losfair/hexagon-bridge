use std::os::raw::c_char;
use std::any::Any;
use std::ffi::CString;
use std::collections::{HashMap, HashSet};
use smallvec::SmallVec;
use hexagon_vm_core::executor::ExecutorImpl;
use hexagon_vm_core::object::Object;
use hexagon_vm_core::object_pool::ObjectPool;
use hexagon_vm_core::value::Value;
use hexagon_vm_core::errors::VMError;

pub type Destructor = extern "C" fn (data: *const ());
pub type OnCall = extern "C" fn (ret_place: *mut Value, data: *const (), n_args: u32, args: *const Value) -> i32;
pub type OnGetField = extern "C" fn (ret_place: *mut Value, data: *const (), field_name: *const c_char) -> i32;
pub type OnSetField = extern "C" fn (data: *const (), field_name: *const c_char, value: *const Value) -> i32;
pub type OnTypename = extern "C" fn (data: *const ()) -> *const c_char;
pub type OnToI64 = extern "C" fn (ret_place: *mut i64, data: *const ()) -> i32;
pub type OnToF64 = extern "C" fn (ret_place: *mut f64, data: *const ()) -> i32;
pub type OnToStr = extern "C" fn (data: *const ()) -> *const c_char;
pub type OnToString = extern "C" fn (data: *const ()) -> *const c_char;
pub type OnToBool = extern "C" fn (ret_place: *mut u32, data: *const ()) -> i32;

pub struct ObjectProxy {
    pub(crate) frozen: bool,
    pub(crate) const_fields: HashSet<String>,
    pub(crate) data: *const (),
    pub(crate) destructor: Option<Destructor>,
    pub(crate) on_call: Option<OnCall>,
    pub(crate) on_get_field: Option<OnGetField>,
    pub(crate) on_set_field: Option<OnSetField>,
    pub(crate) on_typename: Option<OnTypename>,
    pub(crate) on_to_i64: Option<OnToI64>,
    pub(crate) on_to_f64: Option<OnToF64>,
    pub(crate) on_to_str: Option<OnToStr>,
    pub(crate) on_to_string: Option<OnToString>,
    pub(crate) on_to_bool: Option<OnToBool>,
    pub(crate) static_fields: HashMap<String, Value>
}

impl ObjectProxy {
    pub fn new(data: *const ()) -> ObjectProxy {
        ObjectProxy {
            frozen: false,
            const_fields: HashSet::new(),
            data: data,
            destructor: None,
            on_call: None,
            on_get_field: None,
            on_set_field: None,
            on_typename: None,
            on_to_i64: None,
            on_to_f64: None,
            on_to_str: None,
            on_to_string: None,
            on_to_bool: None,
            static_fields: HashMap::new()
        }
    }
}

impl Drop for ObjectProxy {
    fn drop(&mut self) {
        if let Some(f) = self.destructor {
            (f)(self.data);
        }
    }
}

impl Object for ObjectProxy {
    fn get_children(&self) -> Vec<usize> {
        self.static_fields.iter()
            .map(|(k, v)| v)
            .filter(|v| v.is_object())
            .map(|v| v.as_object_id())
            .collect()
    }

    fn as_any(&self) -> &Any {
        self as &Any
    }

    fn as_any_mut(&mut self) -> &mut Any {
        self as &mut Any
    }

    fn call(&self, executor: &mut ExecutorImpl) -> Value {
        if let Some(f) = self.on_call {
            let mut ret_place = Value::Null;

            let frame = executor.get_current_frame();
            let n_args = frame.get_n_arguments();
            let args: SmallVec<[Value; 4]> = (0..n_args).map(|i| frame.get_argument(i).unwrap()).collect();

            ensure_proxied_ok(
                if n_args > 0 {
                    (f)(&mut ret_place, self.data, n_args as u32, &args[0])
                } else {
                    (f)(&mut ret_place, self.data, n_args as u32, ::std::ptr::null())
                }
            );
            ret_place
        } else {
            panic!(VMError::from("Not callable"));
        }
    }

    fn get_field(&self, _pool: &ObjectPool, name: &str) -> Option<Value> {
        if let Some(v) = self.static_fields.get(name) {
            return Some(*v);
        }

        if let Some(f) = self.on_get_field {
            let mut ret_place = Value::Null;

            let mut name: SmallVec<[u8; 32]> = name.as_bytes().into();
            name.push(0);

            ensure_proxied_ok(
                (f)(&mut ret_place, self.data, &name[0] as *const u8 as *const i8)
            );
            Some(ret_place)
        } else {
            panic!(VMError::from("Not implemented"));
        }
    }

    fn has_const_field(&self, _pool: &ObjectPool, name: &str) -> bool {
        if self.frozen {
            true
        } else if self.const_fields.contains(name) {
            true
        } else {
            false
        }
    }
}

fn ensure_proxied_ok(err: i32) {
    if err != 0 {
        panic!(VMError::from("Proxied object returns error"));
    }
}
