//! Sidecar handler scaffold templates.

pub fn render_python(name: &str) -> String {
    format!(
        "#!/usr/bin/env python3\n\"\"\"Sidecar scaffold for {name}.\"\"\"\n\nimport json\nimport sys\n\nSOCKET_PATH = \"/tmp/vil-sidecar-{name}.sock\"\n\n\ndef main() -> int:\n    payload = {{\"handler\": \"{name}\", \"socket\": SOCKET_PATH}}\n    print(json.dumps(payload))\n    return 0\n\n\nif __name__ == \"__main__\":\n    sys.exit(main())\n"
    )
}

pub fn render_go(name: &str) -> String {
    format!(
        "package main\n\nimport (\n    \"encoding/json\"\n    \"fmt\"\n    \"os\"\n)\n\nconst SocketPath = \"/tmp/vil-sidecar-{name}.sock\"\n\nfunc main() {{\n    payload := map[string]string{{\n        \"handler\": \"{name}\",\n        \"socket\": SocketPath,\n    }}\n    data, _ := json.Marshal(payload)\n    fmt.Println(string(data))\n    os.Exit(0)\n}}\n"
    )
}
