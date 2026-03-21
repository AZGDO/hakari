use serde_json::{json, Value};

pub fn get_tool_definitions_openai() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "Read",
                "description": "Returns the content of the specified file.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The file path to read, relative to project root."
                        }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "Grep",
                "description": "Search code intelligently when you know a concept, symbol, or phrase but not the exact file. Prefer this over reading many files. Returns matching lines with context and suggested files when nothing matches.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Text or regex-like query to search for. Use exact identifiers, error text, command names, struct names, CSS classes, or phrases."
                        },
                        "file_glob": {
                            "type": "string",
                            "description": "Optional glob such as '*.rs' or 'src/**/*.rs' to narrow the search."
                        },
                        "context_lines": {
                            "type": "integer",
                            "description": "Optional number of context lines around each match."
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Optional cap on returned matches."
                        }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "Write",
                "description": "Writes the given content to the specified file. Smart behavior includes no-op detection, line-ending preservation, diff previews, related test discovery, and syntax validation for supported types.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "The file path to write to, relative to project root."
                        },
                        "content": {
                            "type": "string",
                            "description": "The full content to write to the file."
                        }
                    },
                    "required": ["path", "content"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "Execute",
                "description": "Runs the given shell command and returns stdout, stderr, and exit code. Commands run from the project root directory.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute."
                        }
                    },
                    "required": ["command"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "SummonNano",
                "description": "Delegates a sub-task to another agent with its own context. The sub-agent works independently on the specified files. Returns the result when the sub-agent completes.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "task": {
                            "type": "string",
                            "description": "Description of the sub-task for the spawned agent."
                        },
                        "files": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of file paths the sub-agent should work with."
                        }
                    },
                    "required": ["task", "files"]
                }
            }
        }),
    ]
}

pub fn get_tool_definitions_anthropic() -> Vec<Value> {
    vec![
        json!({
            "name": "Read",
            "description": "Returns the content of the specified file.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The file path to read, relative to project root."
                    }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "Grep",
            "description": "Search code intelligently when you know a concept, symbol, or phrase but not the exact file. Prefer this over reading many files. Returns matching lines with context and suggested files when nothing matches.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Text or regex-like query to search for. Use exact identifiers, error text, command names, struct names, CSS classes, or phrases."
                    },
                    "file_glob": {
                        "type": "string",
                        "description": "Optional glob such as '*.rs' or 'src/**/*.rs' to narrow the search."
                    },
                    "context_lines": {
                        "type": "integer",
                        "description": "Optional number of context lines around each match."
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Optional cap on returned matches."
                    }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "Write",
            "description": "Writes the given content to the specified file. Smart behavior includes no-op detection, line-ending preservation, diff previews, related test discovery, and syntax validation for supported types.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The file path to write to, relative to project root."
                    },
                    "content": {
                        "type": "string",
                        "description": "The full content to write to the file."
                    }
                },
                "required": ["path", "content"]
            }
        }),
        json!({
            "name": "Execute",
            "description": "Runs the given shell command and returns stdout, stderr, and exit code. Commands run from the project root directory.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute."
                    }
                },
                "required": ["command"]
            }
        }),
        json!({
            "name": "SummonNano",
            "description": "Delegates a sub-task to another agent with its own context. The sub-agent works independently on the specified files. Returns the result when the sub-agent completes.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Description of the sub-task for the spawned agent."
                    },
                    "files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of file paths the sub-agent should work with."
                    }
                },
                "required": ["task", "files"]
            }
        }),
    ]
}
