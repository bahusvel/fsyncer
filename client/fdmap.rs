use std::collections::HashMap;

type FdMap = HashMap<i32, i32>;

#[no_mangle]
pub extern "C" fn fdmap_new() -> *mut FdMap{
    Box::into_raw(Box::new(HashMap::new()))
}

#[no_mangle]
pub extern "C" fn fdmap_free(map: *mut FdMap) {
    drop(unsafe {Box::from_raw(map)});
}

#[no_mangle]
pub extern "C" fn fdmap_set(map: *mut FdMap, key: i32, value: i32) {
    let m = unsafe{&mut *map};
    m.insert(key, value);
}

#[no_mangle]
pub extern "C" fn fdmap_get(map: *mut FdMap, key: i32) -> i32 {
    let m = unsafe{&*map};
    *m.get(&key).unwrap_or(&-1)
}
