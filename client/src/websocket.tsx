export function make_websocket(path: string) {
    const ws_protocol = (window.location.protocol === "https:") ? "wss://" : "ws://";
    const uri = ws_protocol + window.location.host + path;
    return new WebSocket(uri);
}

