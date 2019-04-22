use tide::middleware::{ Middleware, Next };
use tide::cookies::ExtractCookies;
use tide::{ Context, Response };
use futures::future::FutureObj;
use std::collections::HashMap;
use http::header::{ HeaderValue, HeaderMap };
use std::cell::{ RefCell, Ref, RefMut };
use std::ops::{ Deref, DerefMut };
use std::sync::Arc;

#[derive(Clone)]
pub struct SessionMap {
    is_dirty: bool,
    data: HashMap<String, String> // XXX: this could be made more generic / better!
}

// Provide associated functions a la Box or Arc, so we can
// Deref directly to the internal HashMap.
impl SessionMap {
    fn dirty(target: &Ref<Box<Self>>) -> bool {
        target.is_dirty
    }

    pub fn rotate(target: &mut Self) {
        target.is_dirty = true
    }

    pub fn new() -> Self {
        Self {
            is_dirty: false,
            data: HashMap::new()
        }
    }
}

impl Deref for SessionMap {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl DerefMut for SessionMap {
    // XXX: A tweet linked to this line in master earlier. If you're
    // coming in from that link, the original comment is preserved
    // at this URL: https://github.com/chrisdickinson/blog-rs/blob/6dfbe91a4fa09714ce6a975e4663e3e1efdaf9fa/src/session.rs#L45
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

pub trait SessionStore {
    fn load_session(&self, key: &str) -> SessionMap;
    fn create_session(&self) -> SessionMap {
        SessionMap::new()
    }
    fn commit(&self, session: Ref<Box<SessionMap>>) -> Result<HeaderValue, std::io::Error>;
}

pub struct SessionMiddleware<Store: SessionStore + Send + Sync> {
    pub session_key: String,
    pub store: Store
}

#[derive(Clone)]
pub struct SessionCell(RefCell<Box<SessionMap>>);

// We're copying actix, here. I need to understand this better, because
// this strikes me as dangerous.
#[doc(hidden)]
unsafe impl Send for SessionCell {}
#[doc(hidden)]
unsafe impl Sync for SessionCell {}

// If a handler needs access to the session (mutably or immutably) it can
// import this trait.
pub trait SessionExt {
    fn session(&self) -> Ref<Box<SessionMap>>;
    fn session_mut(&self) -> RefMut<Box<SessionMap>>;
}

impl<
    Data: Clone + Send + Sync + 'static
> SessionExt for Context<Data> {
    fn session(&self) -> Ref<Box<SessionMap>> {
        let session_cell = self.extensions().get::<Arc<SessionCell>>().unwrap();
        session_cell.0.borrow()
    }

    fn session_mut(&self) -> RefMut<Box<SessionMap>> {
        let session_cell = self.extensions().get::<Arc<SessionCell>>().unwrap();
        session_cell.0.borrow_mut()
    }
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

            // Create a ref-counted cell (yay interior mutability.) Attach
            // a clone of that arc'd cell to the context and send it
            // through. At the same time, keep our local copy of the arc
            // ready for inspection after we're done processing the
            // request. 
            let cell = Arc::new(SessionCell(RefCell::new(Box::new(session))));
            ctx.extensions_mut().insert(cell.clone());
            let mut res = await!(next.run(ctx));

            // Borrow the session map and check to see if we need to commit
            // it and/or send a new cookie.
            let session_cell = &cell.0;
            let session = session_cell.borrow();
            if !SessionMap::dirty(&session) {
                return res
            }

            if let Ok(key) = self.store.commit(session) {
                if !has_session {
                    let mut hm = res.headers_mut();
                    hm.insert("Set-Cookie", key);
                }
            }

            res
        }))
    }
}
