use serde_json::{json, Value};
use std::path::Path;
use tess_ent::bundle::make_entry_bundle;
use tess_ent::ctx::{js_stringify, js_stringify_pretty, Diag, Options};
use tess_ent::index::compile_project;
use tess_ent::parser::parse_source;
use tess_ent::validate::validate;

const USAGE: &str = "사용법
  tess-ent check <파일.tess>            문법·의미 검사 (컴파일 테스트 포함)
  tess-ent build <파일.tess> [-o 출력]  엔트리 작품(.ent)으로 컴파일
  tess-ent ast   <파일.tess>            AST 를 JSON 으로 출력
  tess-ent ref [language|compile|workflow]  AI 용 Tess 언어 레퍼런스 출력
  tess-ent mcp                          MCP(stdio) 서버로 실행

옵션
  -o, --out <경로>   출력 경로. .json 이면 project.json, 그 외에는 .ent 묶음 (기본: <파일>.ent)
  --assets <폴더>    모양·소리 파일을 찾을 폴더 (여러 번 지정 가능, 기본: 소스 파일이 있는 폴더)
  --name <이름>      작품 이름 (기본: project 블록의 title)
  --strict           에러가 하나라도 있으면 중단하고, '주의'를 '경고'로 올립니다
  --json             진단 결과를 JSON 으로 출력 (check/build)";

struct Cli {
    out: Option<String>,
    assets: Vec<String>,
    name: Option<String>,
    strict: bool,
    json: bool,
    rest: Vec<String>,
}

fn parse_args(args: Vec<String>) -> Cli {
    let mut cli = Cli { out: None, assets: vec![], name: None, strict: false, json: false, rest: vec![] };
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" | "--out" => cli.out = it.next(),
            "--assets" => {
                if let Some(v) = it.next() {
                    cli.assets.push(v)
                }
            }
            "--name" => cli.name = it.next(),
            "--strict" => cli.strict = true,
            "--json" => cli.json = true,
            _ => cli.rest.push(a),
        }
    }
    cli
}

fn absolute(p: &str) -> String {
    tess_ent::assets::normalize(Path::new(p))
}

fn options_for(file: &str, cli: &Cli) -> Options {
    let asset_dirs = if cli.assets.is_empty() {
        vec![Path::new(&absolute(file)).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| ".".into())]
    } else {
        cli.assets.iter().map(|d| absolute(d)).collect()
    };
    Options {
        path: absolute(file),
        asset_dirs,
        name: cli.name.clone(),
        strict: cli.strict,
        seed: None,
        asset_urls: false,
    }
}

fn diag_json(d: &Diag) -> Value {
    json!({"line": d.line, "column": d.column, "message": d.message, "file": d.file})
}

fn print_report(label: &str, items: &[Diag], kind: &str) {
    if items.is_empty() {
        return;
    }
    eprintln!("{} {}개", kind, items.len());
    for d in items.iter().take(8) {
        let where_ = match &d.file {
            Some(f) if Path::new(f).file_name().map(|n| n.to_string_lossy() != label).unwrap_or(false) => {
                Path::new(f).file_name().unwrap().to_string_lossy().into_owned()
            }
            _ => label.to_string(),
        };
        eprintln!("  {}:{}:{}  {}", where_, d.line, d.column, d.message);
        if let Some(detail) = &d.detail {
            for line in detail.lines() {
                eprintln!("    {}", line);
            }
        }
    }
    if items.len() > 8 {
        eprintln!("  … 외 {}개", items.len() - 8);
    }
}

fn label_of(file: &str) -> String {
    Path::new(file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| file.to_string())
}

fn read_source(file: &str) -> Result<String, String> {
    std::fs::read(file)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| format!("{}: {}", file, e))
}

fn run_check(file: &str, cli: &Cli) -> i32 {
    let source = match read_source(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };
    let label = label_of(file);
    let result = compile_project(&source, options_for(file, cli));
    if cli.json {
        let out = json!({
            "ok": result.ok,
            "errors": result.errors.iter().map(diag_json).collect::<Vec<_>>(),
            "warnings": result.warnings.iter().map(diag_json).collect::<Vec<_>>(),
            "notices": result.notices.iter().map(diag_json).collect::<Vec<_>>(),
        });
        println!("{}", js_stringify_pretty(&out));
        return if result.ok { 0 } else { 1 };
    }
    print_report(&label, &result.errors, "에러");
    print_report(&label, &result.warnings, "경고");
    print_report(&label, &result.notices, "주의");
    if !result.ok {
        eprintln!("{}: 에러 {}개", label, result.errors.len());
        return 1;
    }
    println!("OK  {}", label);
    0
}

fn count_blocks(v: &Value) -> usize {
    match v {
        Value::Array(a) => a.iter().map(count_blocks).sum(),
        Value::Object(m) => {
            let own = if m.contains_key("type") && m.contains_key("params") { 1 } else { 0 };
            own + m.get("params").map(count_blocks).unwrap_or(0) + m.get("statements").map(count_blocks).unwrap_or(0)
        }
        _ => 0,
    }
}

fn run_build(file: &str, cli: &Cli) -> i32 {
    let source = match read_source(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };
    let label = label_of(file);
    let result = compile_project(&source, options_for(file, cli));
    if !cli.json {
        print_report(&label, &result.warnings, "경고");
        print_report(&label, &result.notices, "주의");
    }
    let mut failed = false;
    if !result.ok {
        if !cli.json {
            print_report(&label, &result.errors, "에러");
        }
        if cli.strict || result.project.is_none() {
            if cli.json {
                println!(
                    "{}",
                    js_stringify_pretty(&json!({
                        "ok": false,
                        "errors": result.errors.iter().map(diag_json).collect::<Vec<_>>(),
                        "warnings": result.warnings.iter().map(diag_json).collect::<Vec<_>>(),
                    }))
                );
            } else {
                eprintln!("{}: 에러 {}개로 인해 내보낼 수 없습니다.", label, result.errors.len());
            }
            return 1;
        }
        failed = true;
        if !cli.json {
            eprintln!("에러 {}개가 난 문장을 빼고 내보냅니다. --strict 를 붙이면 여기서 멈춥니다.", result.errors.len());
        }
    }
    let project = result.project.clone().unwrap();
    let out_path = cli.out.clone().unwrap_or_else(|| format!("{}.ent", file.strip_suffix(".tess").unwrap_or(file)));
    if let Some(parent) = Path::new(&out_path).parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let as_json = out_path.ends_with(".json");
    let written = if as_json {
        std::fs::write(&out_path, js_stringify_pretty(&project)).map_err(|e| e.to_string())
    } else {
        make_entry_bundle(&js_stringify(&project), &result.assets)
            .and_then(|bytes| std::fs::write(&out_path, bytes).map_err(|e| e.to_string()))
    };
    if let Err(e) = written {
        eprintln!("내보내지 못했습니다: {}", e);
        return 1;
    }
    let blocks: usize = project["objects"]
        .as_array()
        .map(|objs| {
            objs.iter()
                .map(|o| {
                    let script: Value = serde_json::from_str(o["script"].as_str().unwrap_or("[]")).unwrap_or(Value::Null);
                    count_blocks(&script)
                })
                .sum()
        })
        .unwrap_or(0);
    let count = |k: &str| project[k].as_array().map(|a| a.len()).unwrap_or(0);
    if cli.json {
        println!(
            "{}",
            js_stringify_pretty(&json!({
                "ok": !failed,
                "output": out_path,
                "summary": {
                    "scenes": count("scenes"), "objects": count("objects"), "variables": count("variables"),
                    "messages": count("messages"), "functions": count("functions"), "blocks": blocks,
                },
                "errors": result.errors.iter().map(diag_json).collect::<Vec<_>>(),
                "warnings": result.warnings.iter().map(diag_json).collect::<Vec<_>>(),
                "notices": result.notices.iter().map(diag_json).collect::<Vec<_>>(),
            }))
        );
    } else {
        println!(
            "장면 {} · 오브젝트 {} · 변수 {} · 신호 {} · 함수 {} · 블록 {}",
            count("scenes"), count("objects"), count("variables"), count("messages"), count("functions"), blocks
        );
        println!("완료  {} -> {}", label, out_path);
    }
    0
}

fn run_ast(file: &str) -> i32 {
    let source = match read_source(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            return 1;
        }
    };
    let label = label_of(file);
    match parse_source(&source) {
        Err(f) => {
            let (line, column) = tess_ent::parser::line_and_column(&source, f.offset);
            print_report(&label, &[Diag { line, column, offset: f.offset, message: f.message, file: None, detail: Some(f.detail) }], "에러");
            1
        }
        Ok(ast) => {
            let v = validate(&ast, &source, None);
            print_report(&label, &v.errors, "에러");
            print_report(&label, &v.warnings, "경고");
            println!("{}", js_stringify_pretty(&ast));
            if v.errors.is_empty() { 0 } else { 1 }
        }
    }
}

/// Differential-test output: the compile result as one JSON document.
fn run_dump(file: &str, cli: &Cli) -> i32 {
    let source = read_source(file).unwrap_or_default();
    let result = compile_project(&source, options_for(file, cli));
    let diags = |v: &[Diag]| Value::Array(v.iter().map(diag_json).collect());
    let assets: Vec<Value> = result.assets.iter().map(|a| json!({"source": a.source, "target": a.target})).collect();
    let out = json!({
        "ok": result.ok,
        "project": result.project.clone().unwrap_or(Value::Null),
        "errors": diags(&result.errors),
        "warnings": diags(&result.warnings),
        "notices": diags(&result.notices),
        "assets": assets,
    });
    print!("{}", js_stringify(&out));
    0
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        println!("{}", USAGE);
        std::process::exit(if args.is_empty() { 1 } else { 0 });
    }
    let command = args.remove(0);
    let cli = parse_args(args);
    let file = cli.rest.first().cloned();
    let code = match (command.as_str(), file) {
        ("check", Some(f)) => run_check(&f, &cli),
        ("build", Some(f)) => run_build(&f, &cli),
        ("ast", Some(f)) => run_ast(&f),
        ("dump", Some(f)) => run_dump(&f, &cli),
        ("ref", _) => {
            let topic = cli.rest.first().map(|s| s.as_str()).unwrap_or("language");
            print!(
                "{}",
                match topic {
                    "compile" => tess_ent::mcp::COMPILE_REF,
                    "workflow" => tess_ent::mcp::WORKFLOW,
                    _ => tess_ent::mcp::LANGUAGE_REF,
                }
            );
            0
        }
        ("mcp", _) => {
            tess_ent::mcp::serve();
            0
        }
        _ => {
            eprintln!("{}", USAGE);
            1
        }
    };
    std::process::exit(code);
}
