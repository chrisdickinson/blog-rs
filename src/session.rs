use tide::middleware::{ Middleware, Next };
use tide::cookies::ExtractCookies;
use tide::{ Context, Response };
use futures::future::FutureObj;
use http::header::{ HeaderValue, HeaderMap };

struct SessionMap {
    // TODO: keep an internal hashmap of keys -> values for the
    // purpose of tracking changes.
}

impl SessionMap {
    fn dirty(&self) -> bool {
        // TODO: track changes to the internal map, and if any
        // keys have been added/removed/modified, mark the map
        // as dirty.
        false
    }
}

trait SessionStore {
    fn load_session(&self, key: &str) -> SessionMap;
    fn create_session(&self) -> SessionMap;
    fn commit(&self, session: SessionMap) -> Result<HeaderValue, std::io::Error>;
}

struct SessionMiddleware<Store: SessionStore + Send + Sync> {
    session_key: String,
    store: Store
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

            ctx.extensions_mut().insert(session);
            let mut res = await!(next.run(ctx));

            if let Some(session) = res.extensions_mut().remove::<SessionMap>() {
                if !session.dirty() {
                    return res
                }

                if let Ok(key) = self.store.commit(session) {
                    let mut hm = res.headers_mut();
                    hm.insert("Set-Cookie", key);
                }
            }

            res
        }))
    }
}
