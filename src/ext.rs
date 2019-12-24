//! Extension module, gluing together & enhancing the third-party libraries.

pub mod futures {
    use futures::Future;


    /// Reintroduced `BoxFuture` (which is deprecated in new versions of futures-rs).
    ///
    /// Note that unlike the original, it does NOT have the Send requirement,
    /// and also allows to fine-tune the actual lifetime of the future
    /// (rather that requiring it to be 'static).
    pub type BoxFuture<'f, T, E> = Box<dyn Future<Item = T, Error = E> + 'f>;

    /// Trait with additional methods for Future objects.
    pub trait FutureExt: Future + Sized {
        fn into_box<'f>(self) -> BoxFuture<'f, Self::Item, Self::Error>
            where Self: 'f;
    }

    impl<F: Future> FutureExt for F {
        fn into_box<'f>(self) -> BoxFuture<'f, Self::Item, Self::Error>
            where Self: 'f
        {
            Box::new(self)
        }
    }
}

pub mod hyper {
    use futures::{future, Stream};
    use hyper::{Body, Error};

    use ext::futures::{BoxFuture, FutureExt};


    /// Trait with additional methods for the Hyper Body object.
    pub trait BodyExt {
        fn into_bytes(self) -> BoxFuture<'static, Vec<u8>, Error>;
    }

    impl BodyExt for Body {
        fn into_bytes(self) -> BoxFuture<'static, Vec<u8>, Error> {
            self.fold(vec![], |mut buf, chunk| {
                buf.extend_from_slice(&*chunk);
                future::ok::<_, Error>(buf)
            }).into_box()
        }
    }
}
