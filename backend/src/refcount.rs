use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A refcount which represents the number of execution contexts concurrently
/// relying on the existence of a resource.
///
/// Once the refcount reaches 0, the resource is considered released and can no
/// longer be referenced.
///
/// Each interested context can call `try_get` to increase the refcount and
/// thereby represent its expectation that this resource stays alive. If the
/// refcount has already reached 0, `try_get` will fail.
///
/// Calling `try_release` will mark the resource as released by reducing the
/// refcount to 0, however if additional execution contexts hold a reference to
/// the resource, this will fail.
#[derive(Clone)]
pub struct Refcount {
  refs: Arc<AtomicUsize>,
}

impl Refcount {
  pub fn new() -> Self {
    Self {
      refs: Arc::new(AtomicUsize::new(1)),
    }
  }

  pub fn try_get(&self) -> Option<Ref> {
    let mut refs = self.refs.load(Ordering::Relaxed);

    loop {
      if refs == 0 {
        return None;
      }

      match self
        .refs
        .compare_exchange_weak(refs, refs + 1, Ordering::Relaxed, Ordering::Relaxed)
      {
        Ok(_) => break,
        Err(value) => refs = value,
      }
    }

    Some(Ref {
      refs: Arc::clone(&self.refs),
    })
  }

  pub fn try_release(&self) -> bool {
    self
      .refs
      .compare_exchange(1, 0, Ordering::Relaxed, Ordering::Relaxed)
      .is_ok()
  }
}

pub struct Ref {
  refs: Arc<AtomicUsize>,
}

impl Drop for Ref {
  fn drop(&mut self) {
    self.refs.fetch_sub(1, Ordering::Relaxed);
  }
}
