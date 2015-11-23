extern crate rand;
extern crate rustc_serialize;
extern crate time;
extern crate tiny_http;
extern crate url;

pub use assets::match_assets;
pub use log::LogEntry;
pub use input::{SessionsManager, Session, generate_session_id};

use std::io;
use std::io::Cursor;
use std::io::Read;
use std::error;
use std::fmt;
use std::fs::File;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;

pub mod input;

mod assets;
mod find_route;
mod log;
mod router;

/// An error that one of your routes can return.
///
/// This is just a convenience enum and you don't need to use it in your project
/// if you don't want to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteError {
    /// Couldn't find a way to handle this request.
    NoRouteFound,

    /// The user input is wrong.
    WrongInput,

    /// The user must be logged in.
    LoginRequired,

    /// The user entered a wrong login or password.
    WrongLoginPassword,

    /// The user is logged in but shouldn't be there.
    NotAuthorized,
}

impl error::Error for RouteError {
    fn description(&self) -> &str {
        match self {
            &RouteError::NoRouteFound => {
                "Couldn't find a way to handle this request."
            },
            &RouteError::WrongInput => {
                "The body of the request is malformed or missing something."
            },
            &RouteError::LoginRequired => {
                "The client must be logged in before this request can be answered."
            },
            &RouteError::WrongLoginPassword => {
                "The client attempted to login but entered a wrong login or password."
            },
            &RouteError::NotAuthorized => {
                "The client is logged in but doesn't have the permission to access this resource."
            },
        }
    }
}

impl fmt::Display for RouteError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}", error::Error::description(self))
    }
}

/// Starts a server and uses the given requests handler.
///
/// The request handler takes a `&Request` and must return a `Response` to send to the user.
///
/// # Common mistakes
///
/// The handler must capture its environment by value and not by reference (`'static`). If you
/// use closure, don't forget to put `move` in front of the closure.
///
/// The handler must also be thread-safe (`Send` and `Sync`).
/// For example this handler isn't thread-safe:
///
/// ```ignore
/// let mut requests_counter = 0;
///
/// rouille::start_server("localhost:80", move |request| {
///     requests_counter += 1;
///
///     // rest of the handler
/// })
/// ```
///
/// Multiple requests can be processed simultaneously, therefore you can't mutably access
/// variables from the outside.
///
/// Instead you must use a `Mutex`:
///
/// ```no_run
/// use std::sync::Mutex;
/// let requests_counter = Mutex::new(0);
///
/// rouille::start_server("localhost:80", move |request| {
///     *requests_counter.lock().unwrap() += 1;
///
///     // rest of the handler
/// # panic!()
/// })
/// ```
///
/// # Panic handling
///
/// If your request handler panicks, a 500 error will automatically be sent to the client.
///
pub fn start_server<A, F>(addr: A, handler: F) -> !
                          where A: ToSocketAddrs,
                                F: Send + Sync + 'static + Fn(&Request) -> Response
{
    let server = tiny_http::ServerBuilder::new().with_port(8000).build().unwrap();

    for mut request in server.incoming_requests() {
        // TODO: don't read the body in memory immediately
        let mut data = Vec::with_capacity(request.body_length().unwrap_or(0));
        request.as_reader().read_to_end(&mut data);     // TODO: handle error

        // building the `Request` object
        let rouille_request = Request {
            url: request.url().to_owned(),
            method: request.method().as_str().to_owned(),
            headers: request.headers().iter().map(|h| (h.field.to_string(), h.value.clone().into())).collect(),
            https: false,
            data: data,
            remote_addr: request.remote_addr().clone(),
        };

        // calling the handler ; this most likely takes a lot of time
        let mut rouille_response = handler(&rouille_request);

        // writing the response
        let mut response = tiny_http::Response::empty(rouille_response.status_code)
                    .with_data(rouille_response.data.data, rouille_response.data.data_length);

        for (key, value) in rouille_response.headers {
            if let Ok(header) = tiny_http::Header::from_bytes(key, value) {
                response.add_header(header);
            } else {
                // TODO: ?
            }
        }

        request.respond(response);
    }

    unreachable!()
}

/// Represents a request that your handler must answer to.
///
/// This can be either a real request (received by the HTTP server) or a mock object created with
/// one of the `fake_*` constructors.
pub struct Request {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    https: bool,
    data: Vec<u8>,
    remote_addr: SocketAddr,
}

impl Request {
    /// Builds a fake HTTP request to be used during tests.
    ///
    /// The remote address of the client will be `127.0.0.1:12345`. Use `fake_http_from` to
    /// specify what the client's address should be.
    pub fn fake_http<U, M>(method: M, url: U, headers: Vec<(String, String)>, data: Vec<u8>)
                           -> Request where U: Into<String>, M: Into<String>
    {
        Request {
            url: url.into(),
            method: method.into(),
            https: false,
            data: data,
            headers: headers,
            remote_addr: "127.0.0.1:12345".parse().unwrap(),
        }
    }

    /// Builds a fake HTTP request to be used during tests.
    pub fn fake_http_from<U, M>(from: SocketAddr, method: M, url: U,
                                headers: Vec<(String, String)>, data: Vec<u8>)
                                -> Request where U: Into<String>, M: Into<String>
    {
        Request {
            url: url.into(),
            method: method.into(),
            https: false,
            data: data,
            headers: headers,
            remote_addr: from,
        }
    }

    /// Builds a fake HTTPS request to be used during tests.
    ///
    /// The remote address of the client will be `127.0.0.1:12345`. Use `fake_https_from` to
    /// specify what the client's address should be.
    pub fn fake_https<U, M>(method: M, url: U, headers: Vec<(String, String)>, data: Vec<u8>)
                            -> Request where U: Into<String>, M: Into<String>
    {
        Request {
            url: url.into(),
            method: method.into(),
            https: true,
            data: data,
            headers: headers,
            remote_addr: "127.0.0.1:12345".parse().unwrap(),
        }
    }

    /// Builds a fake HTTPS request to be used during tests.
    pub fn fake_https_from<U, M>(from: SocketAddr, method: M, url: U,
                                 headers: Vec<(String, String)>, data: Vec<u8>)
                                 -> Request where U: Into<String>, M: Into<String>
    {
        Request {
            url: url.into(),
            method: method.into(),
            https: true,
            data: data,
            headers: headers,
            remote_addr: from,
        }
    }

    /// Returns `true` if the request uses HTTPS instead of HTTP.
    #[inline]
    pub fn secure(&self) -> bool {
        self.https
    }

    /// Returns the method of the request (`GET`, `POST`, etc.).
    #[inline]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Returns the URL requested by the client. It is not decoded and thus can contain strings
    /// such as `%20`.
    ///
    /// See also `url()`.
    #[inline]
    pub fn raw_url(&self) -> &str {
        &self.url
    }

    /// Returns the URL requested by the client.
    ///
    /// Contrary to `raw_url`, special characters have been decoded.
    /// If there is any non-unicode character in the URL, it will be replaced with `U+FFFD`.
    pub fn url(&self) -> String {
        url::percent_encoding::lossy_utf8_percent_decode(self.url.as_bytes())
    }

    /// Returns the value of a header of the request.
    ///
    /// Returns `None` if no such header could be found.
    #[inline]
    pub fn header(&self, key: &str) -> Option<String> {
        self.headers.iter().find(|&&(ref k, _)| k == key).map(|&(_, ref v)| v.clone())
    }

    /// UNSTABLE. Returns the body of the request.
    ///
    /// Will eventually return an object that implements `Read` instead of a `Vec<u8>`.
    pub fn data(&self) -> Vec<u8> {
        self.data.clone()
    }

    /// Returns the address of the client that made this request.
    #[inline]
    pub fn remote_addr(&self) -> &SocketAddr {
        &self.remote_addr
    }
}

/// Contains a prototype of a response.
/// The response is only sent when you call `Request::respond`.
pub struct Response {
    /// The status code to return to the user.
    pub status_code: u16,

    /// List of headers to be returned in the response.
    ///
    /// Note that important headers such as `Connection` or `Content-Length` will be ignored
    /// from this list.
    // TODO: document precisely which headers
    pub headers: Vec<(String, String)>,

    /// An opaque type that contains the body of the response.
    pub data: ResponseBody,
}

impl Response {
    /// UNSTABLE. Builds a default response to handle the given route error.
    ///
    /// Important: don't use this in a real website. This function is just a convenience when
    /// prototyping.
    ///
    /// For authentication-related errors, you are strongly encouraged to handle them yourself.
    #[inline]
    pub fn from_error(err: &RouteError) -> Response {
        match err {
            &RouteError::NoRouteFound => Response {
                status_code: 404, headers: vec![], data: ResponseBody::empty()
            },
            &RouteError::WrongInput => Response {
                status_code: 400, headers: vec![], data: ResponseBody::empty()
            },
            &RouteError::LoginRequired => Response {
                status_code: 401, headers: vec![], data: ResponseBody::empty()
            },     // TODO: www-auth header?
            &RouteError::WrongLoginPassword => Response {
                status_code: 401, headers: vec![], data: ResponseBody::empty()
            },     // TODO: www-auth header?
            &RouteError::NotAuthorized => Response {
                status_code: 403, headers: vec![], data: ResponseBody::empty()
            },
        }
    }

    /// Builds a `Response` that redirects the user to another URL.
    #[inline]
    pub fn redirect(target: &str) -> Response {
        Response {
            status_code: 303,
            headers: vec![("Location".to_owned(), target.to_owned())],
            data: ResponseBody::empty(),
        }
    }

    /// Builds a `Response` that outputs HTML.
    #[inline]
    pub fn html<D>(content: D) -> Response where D: Into<Vec<u8>> {
        Response {
            status_code: 200,
            headers: vec![("Content-Type".to_owned(), "text/html; charset=utf8".to_owned())],
            data: ResponseBody::from_data(content),
        }
    }

    /// Builds a `Response` that outputs plain text.
    #[inline]
    pub fn text<S>(text: S) -> Response where S: Into<String> {
        Response {
            status_code: 200,
            headers: vec![("Content-Type".to_owned(), "text/plain; charset=utf8".to_owned())],
            data: ResponseBody::from_string(text),
        }
    }

    /// Builds a `Response` that outputs JSON.
    #[inline]
    pub fn json<T>(content: &T) -> Response where T: rustc_serialize::Encodable {
        let data = rustc_serialize::json::encode(content).unwrap();

        Response {
            status_code: 200,
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            data: ResponseBody::from_data(data),
        }
    }

    /// Builds a `Response` that returns a `401 Not Authorized` status
    /// and a `WWW-Authenticate` header.
    #[inline]
    pub fn basic_http_auth_login_required(realm: &str) -> Response {
        // TODO: escape the realm
        Response {
            status_code: 401,
            headers: vec![("WWW-Authenticate".to_owned(), format!("Basic realm=\"{}\"", realm))],
            data: ResponseBody::empty(),
        }
    }

    /// Changes the status code of the response.
    #[inline]
    pub fn with_status_code(mut self, code: u16) -> Response {
        self.status_code = code;
        self
    }
}

/// An opaque type that represents the body of a response.
pub struct ResponseBody {
    data: Box<Read + Send>,
    data_length: Option<usize>,
}

impl ResponseBody {
    /// Builds a `ResponseBody` that doesn't return any data.
    #[inline]
    pub fn empty() -> ResponseBody {
        ResponseBody {
            data: Box::new(io::empty()),
            data_length: Some(0),
        }
    }

    /// Builds a new `ResponseBody` that will read the data from a `Read`.
    ///
    /// Note that this is suboptimal compared to other constructors because the length
    /// isn't known in advance.
    #[inline]
    pub fn from_reader<R>(data: R) -> ResponseBody where R: Read + Send + 'static {
        ResponseBody {
            data: Box::new(data),
            data_length: None,
        }
    }

    /// Builds a new `ResponseBody` that returns the given data.
    #[inline]
    pub fn from_data<D>(data: D) -> ResponseBody where D: Into<Vec<u8>> {
        let data = data.into();
        let len = data.len();

        ResponseBody {
            data: Box::new(Cursor::new(data)),
            data_length: Some(len),
        }
    }

    /// Builds a new `ResponseBody` that returns the content of the given file.
    #[inline]
    pub fn from_file(file: File) -> ResponseBody {
        let len = file.metadata().map(|metadata| metadata.len() as usize).ok();

        ResponseBody {
            data: Box::new(file),
            data_length: len,
        }
    }

    /// Builds a new `ResponseBody` that returns an UTF-8 string.
    #[inline]
    pub fn from_string<S>(data: S) -> ResponseBody where S: Into<String> {
        ResponseBody::from_data(data.into().into_bytes())
    }
}
