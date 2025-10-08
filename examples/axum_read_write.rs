use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use chrono::Local;
use futures_util::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};
use std::time::Duration;

#[tokio::main]
async fn main() {
    // 创建路由
    let app = Router::new()
        .route("/", get(root))
        .route("/ws", get(websocket_handler));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3002")
        .await
        .unwrap();

    println!("🚀 WebSocket 服务器启动在: http://127.0.0.1:3002");
    println!("📡 WebSocket 端点: ws://127.0.0.1:3002/ws");
    println!("\n这个示例演示了读写分离模式：");
    println!("  - write 任务：每2秒自动发送服务器时间");
    println!("  - read 任务：接收并回显客户端消息");

    axum::serve(listener, app).await.unwrap();
}

// 根路由
async fn root() -> impl IntoResponse {
    axum::response::Html(
        r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>WebSocket 读写分离 Demo</title>
    <style>
        body {
            font-family: Arial, sans-serif;
            max-width: 800px;
            margin: 50px auto;
            padding: 20px;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
        }
        .container {
            background: rgba(255, 255, 255, 0.1);
            padding: 30px;
            border-radius: 15px;
            backdrop-filter: blur(10px);
        }
        h1 {
            text-align: center;
            margin-bottom: 30px;
        }
        #messages {
            border: 2px solid rgba(255, 255, 255, 0.3);
            height: 350px;
            overflow-y: auto;
            padding: 15px;
            margin: 20px 0;
            background: rgba(0, 0, 0, 0.3);
            border-radius: 10px;
            font-family: 'Courier New', monospace;
        }
        .message {
            margin: 8px 0;
            padding: 8px;
            border-radius: 5px;
            animation: fadeIn 0.3s;
        }
        @keyframes fadeIn {
            from { opacity: 0; transform: translateY(-10px); }
            to { opacity: 1; transform: translateY(0); }
        }
        .system {
            color: #ffd700;
            font-weight: bold;
        }
        .sent {
            color: #00ff00;
        }
        .received {
            color: #87ceeb;
        }
        .error {
            color: #ff6b6b;
        }
        .input-group {
            display: flex;
            gap: 10px;
        }
        input {
            flex: 1;
            padding: 12px;
            font-size: 16px;
            border: none;
            border-radius: 8px;
            background: rgba(255, 255, 255, 0.9);
        }
        button {
            padding: 12px 30px;
            font-size: 16px;
            border: none;
            border-radius: 8px;
            background: #4CAF50;
            color: white;
            cursor: pointer;
            transition: all 0.3s;
        }
        button:hover {
            background: #45a049;
            transform: scale(1.05);
        }
        button:disabled {
            background: #cccccc;
            cursor: not-allowed;
            transform: scale(1);
        }
        #status {
            text-align: center;
            font-size: 18px;
            margin-bottom: 20px;
            padding: 10px;
            background: rgba(0, 0, 0, 0.3);
            border-radius: 8px;
        }
        .status-connected {
            color: #00ff00;
        }
        .status-disconnected {
            color: #ff6b6b;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>🌐 WebSocket 读写分离演示</h1>
        <div id="status">连接状态: <span id="connection-status" class="status-disconnected">断开 ❌</span></div>
        <div id="messages"></div>
        <div class="input-group">
            <input type="text" id="messageInput" placeholder="输入消息..." disabled>
            <button id="sendBtn" onclick="sendMessage()" disabled>发送 📤</button>
        </div>
    </div>

    <script>
        let ws;
        const messagesDiv = document.getElementById('messages');
        const statusSpan = document.getElementById('connection-status');
        const input = document.getElementById('messageInput');
        const sendBtn = document.getElementById('sendBtn');

        function connect() {
            ws = new WebSocket('ws://' + window.location.host + '/ws');
            
            ws.onopen = () => {
                statusSpan.textContent = '已连接 ✅';
                statusSpan.className = 'status-connected';
                input.disabled = false;
                sendBtn.disabled = false;
                addMessage('系统', '已连接到服务器，开始接收自动消息...', 'system');
            };
            
            ws.onmessage = (event) => {
                addMessage('服务器', event.data, 'received');
            };
            
            ws.onclose = () => {
                statusSpan.textContent = '断开 ❌';
                statusSpan.className = 'status-disconnected';
                input.disabled = true;
                sendBtn.disabled = true;
                addMessage('系统', '连接已断开', 'error');
            };
            
            ws.onerror = (error) => {
                addMessage('系统', '连接错误', 'error');
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
            const time = new Date().toLocaleTimeString();
            div.textContent = `[${time}] ${sender}: ${text}`;
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
async fn websocket_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

// 处理 WebSocket 连接
async fn handle_socket(socket: WebSocket) {
    let (sender, receiver) = socket.split();

    // 启动写任务：定期向客户端发送消息
    let write_task = tokio::spawn(write(sender));

    // 启动读任务：从客户端接收消息
    let read_task = tokio::spawn(read(receiver));

    // 等待任一任务完成
    tokio::select! {
        _ = write_task => println!("✅ 写任务结束"),
        _ = read_task => println!("✅ 读任务结束"),
    }

    println!("👋 连接关闭");
}

// 读任务：从 WebSocket 接收消息
async fn read(mut receiver: SplitStream<WebSocket>) {
    println!("📖 读任务启动");

    // 持续接收来自客户端的消息
    while let Some(msg_result) = receiver.next().await {
        match msg_result {
            Ok(msg) => match msg {
                Message::Text(text) => {
                    println!("📨 收到文本消息: {}", text);
                    // 这里可以处理收到的消息
                    // 例如：解析命令、记录日志等
                }
                Message::Binary(data) => {
                    println!("📦 收到二进制消息: {} 字节", data.len());
                }
                Message::Ping(data) => {
                    println!("🏓 收到 Ping: {:?}", data);
                }
                Message::Pong(data) => {
                    println!("🏓 收到 Pong: {:?}", data);
                }
                Message::Close(frame) => {
                    println!("❌ 收到关闭消息: {:?}", frame);
                    break;
                }
            },
            Err(e) => {
                println!("❌ 接收消息出错: {}", e);
                break;
            }
        }
    }

    println!("📖 读任务结束");
}

// 写任务：向 WebSocket 发送消息
async fn write(mut sender: SplitSink<WebSocket, Message>) {
    println!("✍️  写任务启动");

    let mut interval = tokio::time::interval(Duration::from_secs(2));
    let mut counter = 0;

    // 定期向客户端发送消息
    loop {
        interval.tick().await;
        counter += 1;

        // 创建要发送的消息
        let now = Local::now();
        let message = format!("消息 #{} - 服务器时间: {}", counter, now.format("%H:%M:%S"));

        println!("📤 发送消息: {}", message);

        // 发送消息给客户端
        if let Err(e) = sender.send(Message::Text(message)).await {
            println!("❌ 发送消息失败: {}", e);
            break;
        }

        // 可选：每10条消息后发送一个特殊消息
        if counter % 10 == 0 {
            let special = format!("🎉 里程碑消息！已发送 {} 条消息", counter);
            if let Err(e) = sender.send(Message::Text(special)).await {
                println!("❌ 发送特殊消息失败: {}", e);
                break;
            }
        }
    }

    println!("✍️  写任务结束");
}
