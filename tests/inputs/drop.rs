#![no_std]

/// Tests for Drop trait implementation.
///
/// Drop::drop is emitted as a static `m_drop(*mut Self)` method.
/// The TerminatorKind::Drop in MIR calls this method directly.

pub struct Resource {
    pub value: i32,
}

impl Drop for Resource {
    fn drop(&mut self) {
        let _ = self.value;
    }
}

/// Return the inner value before the resource is dropped at end of scope.
pub fn use_resource(v: i32) -> i32 {
    let r = Resource { value: v };
    r.value * 2
}

/// Two nested drops.
pub fn nested_drop(a: i32, b: i32) -> i32 {
    let r1 = Resource { value: a };
    let r2 = Resource { value: b };
    r1.value + r2.value
}
