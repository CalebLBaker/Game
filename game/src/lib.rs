#![cfg_attr(feature = "strict", deny(warnings))]

use async_trait::async_trait;
use bytes::Buf;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

// Entry Point; Construct WebBrowser object and run game
#[wasm_bindgen]
pub extern "C" fn start() {
    console_error_panic_hook::set_once();
    let platform = WebBrowser::new("http://localhost/");
    if let Some(p) = platform {
        wasm_bindgen_futures::spawn_local(game_lib::run(p));
    }
}

// Future that yields an HtmlImageElement once the element has been fully loaded
struct LoadedImageElement {
    element: Option<web_sys::HtmlImageElement>,
    handler: Option<wasm_bindgen::closure::Closure<dyn FnMut()>>,
}

// Implementation of Future trait for LoadedImageElement
impl std::future::Future for LoadedImageElement {
    type Output = Option<web_sys::HtmlImageElement>;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let future = self.get_mut();
        let element = future.element.as_mut();
        match element {
            Some(e) => {
                if e.complete() {
                    std::task::Poll::Ready(Some(future.element.take().unwrap()))
                } else {
                    // If the element isn't complete, set an onload handler to wake the waker
                    let waker = cx.waker().clone();
                    let closure = Box::new(move || waker.wake_by_ref()) as Box<dyn FnMut()>;
                    future.handler = Some(wasm_bindgen::closure::Closure::wrap(closure));
                    let onload = Some(future.handler.as_ref().unwrap().as_ref().unchecked_ref());
                    e.set_onload(onload);
                    std::task::Poll::Pending
                }
            }
            None => std::task::Poll::Ready(None),
        }
    }
}

// Platform type that abstracts away logic that's specific to a web browser/wasm environment
struct WebBrowser<'a> {
    context: web_sys::CanvasRenderingContext2d,
    web_client: reqwest::Client,
    host: &'a str,
    width: f64,
    height: f64,
}

// Sets the margins and padding on an HtmlElement to 0
fn clear_margin_and_padding(element: &web_sys::HtmlElement) {
    let style = element.style();
    let _ = style.set_property("margin", "0");
    let _ = style.set_property("padding", "0");
}

// Constructor and helper functions for the WebBrowser type
impl<'a> WebBrowser<'a> {
    fn new(host: &'a str) -> Option<WebBrowser> {
        const SIZE_MULTIPLIER: f64 = 0.995;

        // Get handlers for various items from the Html document
        let window = web_sys::window()?;
        let document = window.document()?;
        let canvas_element = document.get_element_by_id("gameCanvas")?;
        let canvas = canvas_element.dyn_ref::<web_sys::HtmlCanvasElement>()?;
        let document_element = document.document_element()?;

        // Clear margin and padding to let the canvas element fill the page
        clear_margin_and_padding(document_element.dyn_ref::<web_sys::HtmlElement>()?);
        clear_margin_and_padding(document.body()?.dyn_ref::<web_sys::HtmlElement>()?);

        // Set the canvas size (I find that the browser creates scroll bars if it fills it exactly,
        // so I use a 0.995 multiplier to avoid the scroll bars)
        let width = window.inner_width().ok()?.as_f64()? * SIZE_MULTIPLIER;
        let height = window.inner_height().ok()?.as_f64()? * SIZE_MULTIPLIER;
        canvas.set_width(width as u32);
        canvas.set_height(height as u32);

        // Create the WebBrowser object
        let context_object = canvas.get_context("2d").ok()??;
        let context = context_object
            .dyn_into::<web_sys::CanvasRenderingContext2d>()
            .ok()?;
        let web_client = reqwest::Client::new();
        Some(WebBrowser {
            context,
            web_client,
            host,
            width,
            height,
        })
    }

    async fn get_file_internal(
        &self,
        path: &str,
    ) -> Result<bytes::buf::Reader<bytes::Bytes>, reqwest::Error> {
        let response = self.web_client.get(&(self.host.to_owned() + path)).send();
        Ok(response.await?.bytes().await?.reader())
    }
}

// Implementation of the Platform trait for the WebBrowser type
#[async_trait(?Send)]
impl game_lib::Platform for WebBrowser<'_> {
    type Image = web_sys::HtmlImageElement;

    type File = bytes::buf::Reader<bytes::Bytes>;

    type ImageFuture = LoadedImageElement;

    fn draw(&self, image: &Self::Image, left: f64, top: f64, width: f64, height: f64) {
        let context = &self.context;
        let _ = context
            .draw_image_with_html_image_element_and_dw_and_dh(image, left, top, width, height);
    }

    fn get_width(&self) -> f64 {
        self.width
    }

    fn get_height(&self) -> f64 {
        self.height
    }

    fn get_image(path: &str) -> Self::ImageFuture {
        let element = web_sys::HtmlImageElement::new();
        match element {
            Ok(e) => {
                e.set_src(path);
                LoadedImageElement {
                    element: Some(e),
                    handler: None,
                }
            }
            _ => LoadedImageElement {
                element: None,
                handler: None,
            },
        }
    }

    async fn get_file(&self, path: &str) -> Result<Self::File, String> {
        match self.get_file_internal(path).await {
            Ok(ret) => Ok(ret),
            Err(err) => Err(err.to_string()),
        }
    }

    fn log(msg: &str) {
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(msg));
    }
}
