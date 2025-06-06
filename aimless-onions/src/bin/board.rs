use aimless_onions::shared::{self, Message};
use clap::Parser;
use color_eyre::Result;
use csv::Writer;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::info;
use std::{fs::OpenOptions, path::PathBuf, sync::{Arc, Mutex}};
use warp::{http::Response, Filter};

//To remove the ALLOWED_MESSAGES content filtering, remove this array
const ALLOWED_MESSAGES: [&str; 5] = [
    "Hello",
    "Goodbye",
    "I like Onions!",
    "Wow",
    "This works!"
];

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Config {
    port: u16
}

#[derive(Parser, Debug, Clone)]
struct Cli {
    #[arg(short, long, default_value = "board.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt().init();
    let cli = Cli::parse();
    let config: Config = toml::from_str(&fs::read_to_string(&cli.config).await?)?;

    let human_strings = ALLOWED_MESSAGES.map(|m| format!("<code>'{}'</code>", m));
    let board_html = std::str::from_utf8(include_bytes!("../board.html")).expect("Failed to load board.html")
        .replace("%%MESSAGES%%", &human_strings.join(", "));

    let messages = Arc::new(Mutex::new(Vec::<Message>::new()));

    let relay_route = warp::post()
        .and(warp::path("relay"))
        .and(warp::body::json())
        .and(with_messages(messages.clone()))
        .and_then(handle_post_message);

    let messages_route = warp::get()
        .and(warp::path("messages"))
        .and(with_messages(messages.clone()))
        .and_then(handle_get_messages);

    let frontend_route = warp::get()
        .and(warp::path("board"))
        .and(warp::any().map(move|| board_html.clone()))
        .and_then(handle_frontend);

    let routes = relay_route
        .or(messages_route)
        .or(frontend_route);

    // Run both routes on separate ports
    warp::serve(routes).run(shared::parse_socket_addr("0.0.0.0", config.port).expect("Invalid socket address given")).await;
    Ok(())
}

// Helper function to pass shared state into filter chain
fn with_messages(
    messages: Arc<Mutex<Vec<Message>>>,
) -> impl warp::Filter<Extract = (Arc<Mutex<Vec<Message>>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || messages.clone())
}

async fn handle_post_message(
    message: Message,
    messages: Arc<Mutex<Vec<Message>>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("Received message {}", message.content);
    let travel_time = shared::timestamp() - message.timestamp;

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("timestamps.csv").expect("Could not create timings.csv");
    let mut writer = Writer::from_writer(file);
    writer.serialize((&message.content,&travel_time)).expect("Could not serialize message to timings.csv");
    writer.flush().expect("Could not flush value to timings.csv");
    info!("Message took {}ms. Logged to timings.csv", travel_time);

    //To remove the ALLOWED_MESSAGES content filtering, remove this if statement
    if !ALLOWED_MESSAGES.contains(&message.content.as_str()) {
        let mut messages = messages.lock().unwrap();
        let redacted_message = Message {
            timestamp: 0,
            content: "The Board redacted this message because its content is not one of the known strings.".to_string()
        };
        messages.push(redacted_message);
        if messages.len() > 100 {
            messages.remove(0);
        }
        return Ok(Response::builder().status(400).body("Payload not allowed"))
    }

    let mut messages = messages.lock().unwrap();
    messages.push(message);
    if messages.len() > 100 {
        messages.remove(0);
    }
    Ok(Response::builder().status(200).body("Message received"))
}

async fn handle_get_messages(messages: Arc<Mutex<Vec<Message>>>) -> Result<impl warp::Reply, warp::Rejection> {
    let messages = messages.lock().unwrap();
    let json_messages = serde_json::to_string(&*messages).unwrap();

    Ok(Response::builder().status(200).body(json_messages).unwrap())
}

async fn handle_frontend(board_html: String) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(Response::builder().status(200).body(board_html).unwrap())
}
