use std::path::Path;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use warp::{ws::Message, Filter};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Toolpath {
    lines: Vec<Line>,
    arcs: Vec<ArcMove>,
    rapids: Vec<Line>,
    bounds: Bounds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Line {
    x1: f64, y1: f64, z1: f64,
    x2: f64, y2: f64, z2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArcMove {
    x: f64, y: f64, z: f64,
    i: f64, j: f64,
    start_angle: f64,
    end_angle: f64,
    clockwise: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Bounds {
    min_x: f64, max_x: f64,
    min_y: f64, max_y: f64,
    min_z: f64, max_z: f64,
}

pub async fn runviz(gcode_file: String) {
    let file_path = Arc::new(gcode_file);
    let toolpath = Arc::new(RwLock::new(parse_gcode(&file_path)));
    
    let (tx, _rx) = broadcast::channel(100);
    let tx = Arc::new(tx);
    
    // File watcher
    let watch_path = file_path.clone();
    let watch_toolpath = toolpath.clone();
    let watch_tx = tx.clone();
    
    tokio::spawn(async move {
        let (watcher_tx, mut watcher_rx) = tokio::sync::mpsc::channel(10);
        
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                let _ = watcher_tx.blocking_send(res);
            },
            Config::default(),
        ).unwrap();
        
        watcher.watch(Path::new(&*watch_path), RecursiveMode::NonRecursive).unwrap();
        
        while let Some(res) = watcher_rx.recv().await {
            match res {
                Ok(_event) => {
                    // Debounce
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    
                    // Reparse
                    let new_toolpath = parse_gcode(&watch_path);
                    let mut tp = watch_toolpath.write().await;
                    *tp = new_toolpath.clone();
                    drop(tp);
                    
                    // Notify clients
                    let json = serde_json::to_string(&new_toolpath).unwrap();
                    let _ = watch_tx.send(json);
                }
                Err(e) => eprintln!("Watch error: {:?}", e),
            }
        }
    });
    
    // WebSocket route
    let toolpath_ws = toolpath.clone();
    let tx_ws = tx.clone();
    
    let ws_route = warp::path("ws")
        .and(warp::ws())
        .map(move |ws: warp::ws::Ws| {
            let tp = toolpath_ws.clone();
            let tx = tx_ws.clone();
            
            ws.on_upgrade(move |websocket| {
                handle_websocket(websocket, tp, tx)
            })
        });
    
    // Static files
    let index = warp::path::end().map(|| {
        warp::reply::html(INDEX_HTML)
    });
    
    let routes = ws_route.or(index);
    
    println!("🚀 swarf-viz running at http://localhost:3030");
    println!("📁 Watching: {}", file_path);
    
    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

use futures::{SinkExt, StreamExt};

async fn handle_websocket(
    ws: warp::ws::WebSocket,
    toolpath: Arc<RwLock<Toolpath>>,
    tx: Arc<broadcast::Sender<String>>,
) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    let mut rx = tx.subscribe();
    
    // Send initial state
    let tp = toolpath.read().await.clone();
    let json = serde_json::to_string(&tp).unwrap();
    let _ = ws_tx.send(Message::text(json)).await;
    
    // Forward broadcast messages
    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(json) => {
                        if ws_tx.send(Message::text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            msg = ws_rx.next() => {
                if msg.is_none() {
                    // Client disconnected
                    break;
                }
            }
        }
    }
}

fn parse_gcode(path: &str) -> Toolpath {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    
    let mut lines: Vec<Line> = Vec::new();
    let mut rapids: Vec<Line> = Vec::new();
    let mut arcs: Vec<ArcMove> = Vec::new();
    
    let mut x = 0.0;
    let mut y = 0.0;
    let mut z = 5.0;
    let mut prev_x = 0.0;
    let mut prev_y = 0.0;
    let mut prev_z = 5.0;
    
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut min_z = f64::INFINITY;
    let mut max_z = f64::NEG_INFINITY;
    
    let mut is_rapid = true;
    
    for line in content.lines() {
        let line = line.trim();
        
        // Skip comments and empty lines
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        
        let upper = line.to_uppercase();
        
        // Check for G-codes
        if upper.contains("G00") || upper.contains("G0 ") {
            is_rapid = true;
        } else if upper.contains("G01") || upper.contains("G1 ") {
            is_rapid = false;
        }
        
        // Parse coordinates
        let new_x = parse_coord(line, 'X').unwrap_or(x);
        let new_y = parse_coord(line, 'Y').unwrap_or(y);
        let new_z = parse_coord(line, 'Z').unwrap_or(z);
        
        // Only add line if position changed
        if (new_x - x).abs() > 0.0001 || (new_y - y).abs() > 0.0001 || (new_z - z).abs() > 0.0001 {
            let line_seg = Line {
                x1: prev_x, y1: prev_y, z1: prev_z,
                x2: new_x, y2: new_y, z2: new_z,
            };
            
            if is_rapid {
                rapids.push(line_seg);
            } else {
                lines.push(line_seg);
            }
            
            // Update bounds
            min_x = min_x.min(new_x);
            max_x = max_x.max(new_x);
            min_y = min_y.min(new_y);
            max_y = max_y.max(new_y);
            min_z = min_z.min(new_z);
            max_z = max_z.max(new_z);
            
            prev_x = new_x;
            prev_y = new_y;
            prev_z = new_z;
            x = new_x;
            y = new_y;
            z = new_z;
        }
    }
    
    // Handle case where no moves were found
    if min_x == f64::INFINITY {
        min_x = 0.0; max_x = 100.0;
        min_y = 0.0; max_y = 100.0;
        min_z = 0.0; max_z = 10.0;
    }
    
    Toolpath {
        lines,
        arcs,
        rapids,
        bounds: Bounds { min_x, max_x, min_y, max_y, min_z, max_z },
    }
}

fn parse_coord(line: &str, coord: char) -> Option<f64> {
    // Simple parser: find X/Y/Z followed by number
    let prefix = format!("{}", coord);
    if let Some(pos) = line.to_uppercase().find(&prefix) {
        let rest = &line[pos + 1..];
        // Extract number (including decimal and negative)
        let num_str: String = rest.chars()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_digit(10) || *c == '.' || *c == '-')
            .collect();
        return num_str.parse::<f64>().ok();
    }
    None
}

static INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <title>swarf viz</title>
    <style>
        body { margin: 0; overflow: hidden; background: #1a1a1a; font-family: system-ui, sans-serif; }
        #canvas { width: 100vw; height: 100vh; }
        #info {
            position: fixed; top: 10px; left: 10px;
            color: #fff; background: rgba(0,0,0,0.7);
            padding: 15px; border-radius: 8px;
            font-size: 14px; pointer-events: none;
        }
        #status {
            position: fixed; top: 10px; right: 10px;
            color: #4f4; background: rgba(0,0,0,0.7);
            padding: 8px 15px; border-radius: 8px;
            font-size: 12px;
        }
        .disconnected { color: #f44 !important; }
    </style>
</head>
<body>
    <canvas id="canvas"></canvas>
    <div id="info">
        <strong>swarf viz</strong><br>
        <span id="bounds">Loading...</span><br>
        <span id="stats"></span>
    </div>
    <div id="status">● Live</div>
    
    <script>
        const canvas = document.getElementById('canvas');
        const ctx = canvas.getContext('2d');
        let toolpath = { lines: [], rapids: [], bounds: { min_x: 0, max_x: 100, min_y: 0, max_y: 100 } };
        let scale = 1, offsetX = 0, offsetY = 0;
        let isDragging = false, lastX = 0, lastY = 0;
        
        function resize() {
            canvas.width = window.innerWidth;
            canvas.height = window.innerHeight;
            draw();
        }
        
        function worldToScreen(x, y) {
            return {
                x: (x - toolpath.bounds.min_x) * scale + offsetX,
                y: canvas.height - ((y - toolpath.bounds.min_y) * scale + offsetY)
            };
        }
        
        function fitToView() {
            const padding = 50;
            const w = toolpath.bounds.max_x - toolpath.bounds.min_x;
            const h = toolpath.bounds.max_y - toolpath.bounds.min_y;
            const scaleX = (canvas.width - padding * 2) / w;
            const scaleY = (canvas.height - padding * 2) / h;
            scale = Math.min(scaleX, scaleY);
            offsetX = padding;
            offsetY = padding;
            draw();
        }
        
        function draw() {
            ctx.fillStyle = '#1a1a1a';
            ctx.fillRect(0, 0, canvas.width, canvas.height);
            
            // Draw grid
            ctx.strokeStyle = '#333';
            ctx.lineWidth = 1;
            const gridSize = 10 * scale;
            for (let x = offsetX % gridSize; x < canvas.width; x += gridSize) {
                ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, canvas.height); ctx.stroke();
            }
            for (let y = offsetY % gridSize; y < canvas.height; y += gridSize) {
                ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(canvas.width, y); ctx.stroke();
            }
            
            // Draw rapids (grey)
            ctx.strokeStyle = '#666';
            ctx.lineWidth = 1;
            ctx.setLineDash([5, 5]);
            for (const line of toolpath.rapids) {
                const p1 = worldToScreen(line.x1, line.y1);
                const p2 = worldToScreen(line.x2, line.y2);
                ctx.beginPath(); ctx.moveTo(p1.x, p1.y); ctx.lineTo(p2.x, p2.y); ctx.stroke();
            }
            ctx.setLineDash([]);
            
            // Draw cuts (amber)
            ctx.strokeStyle = '#ffaa00';
            ctx.lineWidth = 2;
            for (const line of toolpath.lines) {
                const p1 = worldToScreen(line.x1, line.y1);
                const p2 = worldToScreen(line.x2, line.y2);
                ctx.beginPath(); ctx.moveTo(p1.x, p1.y); ctx.lineTo(p2.x, p2.y); ctx.stroke();
            }
            
            // Draw start point
            if (toolpath.lines.length > 0) {
                const start = worldToScreen(toolpath.lines[0].x1, toolpath.lines[0].y1);
                ctx.fillStyle = '#0f0';
                ctx.beginPath(); ctx.arc(start.x, start.y, 5, 0, Math.PI * 2); ctx.fill();
            }
            
            // Update info
            const b = toolpath.bounds;
            document.getElementById('bounds').textContent = 
                `X: ${b.min_x.toFixed(1)} to ${b.max_x.toFixed(1)} | Y: ${b.min_y.toFixed(1)} to ${b.max_y.toFixed(1)}`;
            document.getElementById('stats').textContent = 
                `${toolpath.lines.length} cuts, ${toolpath.rapids.length} rapids`;
        }
        
        // Mouse controls
        canvas.addEventListener('mousedown', e => { isDragging = true; lastX = e.clientX; lastY = e.clientY; });
        canvas.addEventListener('mousemove', e => {
            if (isDragging) {
                offsetX += e.clientX - lastX;
                offsetY -= e.clientY - lastY;
                lastX = e.clientX; lastY = e.clientY;
                draw();
            }
        });
        canvas.addEventListener('mouseup', () => isDragging = false);
        canvas.addEventListener('wheel', e => {
            e.preventDefault();
            const factor = e.deltaY > 0 ? 0.9 : 1.1;
            scale *= factor;
            draw();
        });
        
        // WebSocket
        const ws = new WebSocket('ws://localhost:3030/ws');
        ws.onmessage = (event) => {
            toolpath = JSON.parse(event.data);
            fitToView();
        };
        ws.onclose = () => {
            document.getElementById('status').className = 'disconnected';
            document.getElementById('status').textContent = '● Disconnected';
        };
        
        window.addEventListener('resize', resize);
        resize();
    </script>
</body>
</html>"#;
