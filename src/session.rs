use tide::middleware::{ Middleware, Next };
use tide::cookies::ExtractCookies;
use tide::{ Context, Response };
use futures::future::FutureObj;
use http::header::{ HeaderValue, HeaderMap };

pub struct SessionMap {
    // TODO: keep an internal hashmap of keys -> values for the
    // purpose of tracking changes.
}

impl SessionMap {
    fn is_dirty(&self) -> bool {
        false
    }
}

pub trait SessionStore {
    fn load_session(&self, key: &str) -> SessionMap;
    fn create_session(&self) -> SessionMap;
    fn commit(&self, session: SessionMap) -> Result<HeaderValue, std::io::Error>;
}

pub struct SessionMiddleware<Store: SessionStore + Send + Sync> {
    pub session_key: String,
    pub store: Store
}

impl<
    Data: Clone + Send + Sync + 'static,
    S: SessionStore + Send + Sync + 'static
> Middleware<Data> for SessionMiddleware<S> {
    fn handle<'a>(&'a self, mut ctx: Context<Data>, next: Next<'a, Data>) -> FutureObj<'a, Response> {

        FutureObj::new(Box::new(async move {
            let maybe_session = ctx.cookie(&self.session_key);
            let has_session = maybe_session.is_some();

            let mut session = if has_session {
                self.store.load_session(maybe_session.unwrap().value())
            } else {
                self.store.create_session()
            };

            // XXX: I have a problem. To frame it: I want to provide the
            // session map to later middleware & handlers by attaching it to
            // the `Context`'s `Extensions`. However, **after** a response is
            // generated, I want to be able to look at the session object to
            // check whether it's been modified (or otherwise marked "dirty".)
            //
            // The goal is that only once a session is marked dirty, do we do
            // the work of storing the session data in a backing store.
            //
            // Further, we only do the work of sending "Set-Cookie" if we don't
            // already have a session (or if later handlers specifically
            // request it.)
            //
            // The problem is:
            //
            // `Context` is consumed by `next.run(ctx)`, so I can't get back to
            // its extensions (& by extension, the `SessionMap`) in the response
            // phase. I have a false start committed here, at (A) below.
            //
            // I'm going to keep noodling on this, but if you've got an answer
            // I'd really appreciate your thoughts. It'd be most expedient to
            // open an issue here:
            //
            // https://github.com/chrisdickinson/blog-rs/issues/new
            ctx.extensions_mut().insert(session);
            let mut res = await!(next.run(ctx));

            // A) This won't work because `res.extensions` are not the same as
            // `ctx.extensions`.
            if let Some(session) = res.extensions_mut().remove::<SessionMap>() {
                if !session.is_dirty() {
                    return res
                }

                if let Ok(key) = self.store.commit(session) {
                    // TODO: handle manually rotated cookies, like during login/logoff.
                    if !has_session {
                        let mut hm = res.headers_mut();
                        hm.insert("Set-Cookie", key);
                    }
                }
            }

            res
        }))
    }
}
