#![feature(futures_api, async_await, await_macro, existential_type)]

struct XClacks;
use tide::middleware::{ Middleware, Next };
use tide::{ Context, Response };
use std::net::SocketAddr;
use std::env;
use dotenv;
use std::cell::Ref;

use http::{
    header::{HeaderValue, IntoHeaderName},
    HeaderMap, HttpTryFrom,
};

use futures::future::FutureObj;

mod session;
use crate::session::{ SessionMiddleware, SessionStore, SessionMap, SessionExt };

struct InMemorySessionStore;

impl SessionStore for InMemorySessionStore {
    fn load_session(&self, key: &str) -> SessionMap {
        SessionMap::new()
    }

    fn commit(&self, session: Ref<Box<SessionMap>>) -> Result<HeaderValue, std::io::Error> {
        Ok(HeaderValue::from_static("hi"))
    }
}

impl<Data: Clone + Send + Sync + 'static> Middleware<Data> for XClacks {
    fn handle<'a>(&'a self, ctx: Context<Data>, next: Next<'a, Data>) -> FutureObj<'a, Response> {
        FutureObj::new(Box::new(async move {
            let mut res = await!(next.run(ctx));
            let headers = res.headers_mut();

            headers.entry("x-clacks-overhead").unwrap().or_insert_with(
                || HeaderValue::try_from("GNU Terry Pratchett").unwrap()
            );
            res
        }))
    }
}

fn main() {
    let mut app = tide::App::new(());
    app.middleware(XClacks {});
    app.middleware(SessionMiddleware {
        store: InMemorySessionStore { },
        session_key: "sid".to_string()
    });
    dotenv::dotenv().ok();

    app.at("/").get(async move |ctx: Context<()>| {
        let mut sess = ctx.session_mut();

        SessionMap::rotate(&mut sess);

        "Hello, world!"
    });

    let host = env::var("HOST")
        .as_ref()
        .map(String::as_str)
        .unwrap_or("0.0.0.0")
        .to_string();

    let port = env::var("PORT")
        .as_ref()
        .map(String::as_str)
        .unwrap_or("8125")
        .to_string();

    let addr: SocketAddr = format!("{}:{}", host, port).parse().unwrap();
    app.serve(addr);
}
