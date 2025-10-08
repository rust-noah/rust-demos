use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use tokio::sync::broadcast;
use uuid::Uuid;

// 共享状态，用于广播消息
#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<String>,
}

#[tokio::main]

async fn main() {
    // 创建广播通道
    let (tx, _rx) = broadcast::channel(100);

    let app_state = AppState { tx };

    // 创建路由
    let app = Router::new()
        .route("/", get(root))
        .route("/ws", get(websocket_handler))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001")
        .await
        .unwrap();

    println!("🚀 WebSocket 服务器启动在: http://127.0.0.1:3000");
    println!("📡 WebSocket 端点: ws://127.0.0.1:3000/ws");
    println!("\n使用以下命令测试:");
    println!("  websocat ws://127.0.0.1:3000/ws");
    println!("  或使用浏览器控制台:");
    println!("  const ws = new WebSocket('ws://127.0.0.1:3000/ws');");
    println!("  ws.onmessage = (e) => console.log('收到:', e.data);");
    println!("  ws.send('Hello!');");

    axum::serve(listener, app).await.unwrap();
}

// 根路由，返回简单的 HTML 页面
async fn root() -> impl IntoResponse {
    axum::response::Html(
        r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>WebSocket Demo</title>
    <style>
        body {
            font-family: Arial, sans-serif;
            max-width: 800px;
            margin: 50px auto;
            padding: 20px;
        }
        #messages {
            border: 1px solid #ccc;
            height: 300px;
            overflow-y: auto;
            padding: 10px;
            margin: 20px 0;
            background: #f9f9f9;
        }
        .message {
            margin: 5px 0;
            padding: 5px;
        }
        .sent {
            color: blue;
        }
        .received {
            color: green;
        }
        input, button {
            padding: 10px;
            font-size: 16px;
        }
        input {
            width: 70%;
        }
        button {
            width: 25%;
        }
    </style>
</head>
<body>
    <h1>🌐 WebSocket 测试页面</h1>
    <div id="status">连接状态: <span id="connection-status">断开</span></div>
    <div id="messages"></div>
    <div>
        <input type="text" id="messageInput" placeholder="输入消息...">
        <button onclick="sendMessage()">发送</button>
    </div>

    <script>
        let ws;
        const messagesDiv = document.getElementById('messages');
        const statusSpan = document.getElementById('connection-status');
        const input = document.getElementById('messageInput');

        function connect() {
            ws = new WebSocket('ws://' + window.location.host + '/ws');
            
            ws.onopen = () => {
                statusSpan.textContent = '已连接 ✅';
                statusSpan.style.color = 'green';
                addMessage('系统', '已连接到服务器');
            };
            
            ws.onmessage = (event) => {
                addMessage('服务器', event.data, 'received');
            };
            
            ws.onclose = () => {
                statusSpan.textContent = '断开 ❌';
                statusSpan.style.color = 'red';
                addMessage('系统', '连接已断开');
            };
            
            ws.onerror = (error) => {
                addMessage('错误', '连接错误');
            };
        }

        function sendMessage() {
            const message = input.value.trim();
            if (message && ws.readyState === WebSocket.OPEN) {
                ws.send(message);
                addMessage('你', message, 'sent');
                input.value = '';
            }
        }

        function addMessage(sender, text, className = '') {
            const div = document.createElement('div');
            div.className = 'message ' + className;
            div.textContent = `[${new Date().toLocaleTimeString()}] ${sender}: ${text}`;
            messagesDiv.appendChild(div);
            messagesDiv.scrollTop = messagesDiv.scrollHeight;
        }

        input.addEventListener('keypress', (e) => {
            if (e.key === 'Enter') {
                sendMessage();
            }
        });

        // 自动连接
        connect();
    </script>
</body>
</html>
        "#,
    )
}

// WebSocket 升级处理器
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

// 处理 WebSocket 连接
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // 订阅广播通道
    let mut rx = state.tx.subscribe();

    // 生成一个用户ID
    let user_id = Uuid::new_v4().to_string()[..8].to_string();
    println!("🔗 新连接: {}", user_id);

    // 发送欢迎消息
    let welcome = format!("欢迎! 你的ID是: {}", user_id);
    let _ = sender.send(Message::Text(welcome)).await;

    // 发送任务：从广播通道接收消息并发送给客户端
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // 接收任务：从客户端接收消息并广播
    let tx = state.tx.clone();
    let user_id_clone = user_id.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    println!("📨 收到来自 {}: {}", user_id_clone, text);
                    // 广播消息
                    let broadcast_msg = format!("[{}]: {}", user_id_clone, text);
                    let _ = tx.send(broadcast_msg);
                }
                Message::Close(_) => {
                    println!("❌ 断开连接: {}", user_id_clone);
                    break;
                }
                _ => {}
            }
        }
    });

    // 等待任一任务完成
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }

    println!("👋 {} 离开了", user_id);
}
