use covert_sdk::Client;

use crate::handle_resp;

pub async fn handle_status(sdk: &Client) {
    let resp = sdk.status.status().await;
    handle_resp(resp);
}
