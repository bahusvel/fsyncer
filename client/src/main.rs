extern crate common;

use common::*;

#[link(name = "fsyncer_client", kind = "static")]
extern {
    fn do_call(message: *const op_msg) -> i32;
}
