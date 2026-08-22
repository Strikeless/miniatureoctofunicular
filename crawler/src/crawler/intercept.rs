use std::{collections::HashMap, sync::{Arc, RwLock}, time::Instant};

use base64::Engine;
use headless_chrome::protocol::cdp::Network::{GetResponseBodyReturnObject, events::ResponseReceivedEventParams};
use tracing::debug;

#[derive(Debug, Clone)]
pub struct TabNetworkInterceptor {
    intercepted_data_map: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl TabNetworkInterceptor {
    pub fn attach(tab: &headless_chrome::Tab) -> anyhow::Result<Self> {
        let intercepted_data_map = Arc::new(RwLock::default());

        let response_handler = get_browser_response_handler(intercepted_data_map.clone());
        tab.register_response_handling("crawler_response_handler", Box::new(response_handler))?;

        Ok(Self {
            intercepted_data_map,
        })
    }

    pub fn clear(&mut self) {
        let mut intercepted_data_map = self.intercepted_data_map.write().unwrap();
        intercepted_data_map.clear();
    }

    pub fn take(&self, url: &str) -> Option<Vec<u8>> {
        if !self.contains(url) {
            return None;
        }

        let mut intercepted_data_map = self.intercepted_data_map.write().unwrap();
        intercepted_data_map.remove(url)
    }

    pub fn contains(&self, url: &str) -> bool {
        let intercepted_data_map = self.intercepted_data_map.read().unwrap();
        intercepted_data_map.contains_key(url)
    }
}

fn get_browser_response_handler(
    intercepted_data_map: Arc<RwLock<HashMap<String, Vec<u8>>>>
) -> impl for<'a> Fn(
    ResponseReceivedEventParams,
    &(dyn Fn() -> anyhow::Result<GetResponseBodyReturnObject> + 'a)
) {
    move |event_params, fetch_body_fn| {

        let start_fetch_instant = Instant::now();
        let get_response_body_response = fetch_body_fn().expect("Network::GetResponseBody");

        let response_url = event_params.response.url;
        let data = match get_response_body_response.base_64_encoded {
            true => base64::engine::general_purpose::STANDARD
                .decode(get_response_body_response.body)
                .expect("Decoding base64"),
            false => get_response_body_response.body
                .into_bytes(),
        };

        let end_fetch_instant = Instant::now();
        let fetch_duration = end_fetch_instant - start_fetch_instant;

        debug!(
            url = match response_url.starts_with("data:") {
                true => "<data:...>".to_owned(),
                false => response_url.clone(),
            },
            "Intercepted {} bytes (took {} ms)", data.len(), fetch_duration.as_millis()
        );

        let mut intercepted_data_map = intercepted_data_map.write().unwrap();
        intercepted_data_map.insert(response_url, data);
    }
}

/*
let request_interceptor_fn: fn(Arc<Transport>, SessionId, RequestPausedEvent) -> RequestPausedDecision = |transport, session_id, event| {
    let get_response_body_response = transport.call_method_on_target(
        session_id,
        Fetch::GetResponseBody {
            request_id: event.params.request_id.clone(),
        }
    ).expect("Fetch::GetResponseBody");

    let data = match get_response_body_response.base_64_encoded {
        true => base64::engine::general_purpose::STANDARD.decode(get_response_body_response.body).expect("Decoding base64"),
        false => get_response_body_response.body.into_bytes(),
    };

    debug!("Intercepted {} bytes from '{}'.", data.len(), event.params.request.url);

    RequestPausedDecision::Fulfill(FulfillRequest {
        request_id: event.params.request_id.clone(),
        response_code: match event.params.response_status_code {
            Some(response_status_code) => response_status_code,
            None => {
                error!("RequestPausedEvent has no response_status_code. Figure this shit out. Returning 500 for now.");
                500
            }
        },
        response_phrase: event.params.response_status_text,
        response_headers: event.params.response_headers,
        binary_response_headers: None,
        body: Some(base64::engine::general_purpose::STANDARD.encode(&data)),
    })
};
tab.enable_fetch(
    Some(&[
        RequestPattern {
            request_stage: Some(Fetch::RequestStage::Response),
            resource_Type: None,
            url_pattern: None,
        }
    ]),
    None
)?;
tab.enable_request_interception(Arc::new(request_interceptor_fn))?;
*/
