use anyhow::{anyhow, Context, Result};
use clap::Parser;
use chrono::Local;
use directories::ProjectDirs;
// use hostname::get as get_hostname;
use whoami;
use lettre::message::{header::ContentType, Mailbox, Message, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{SmtpTransport, Transport};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};
use std::thread;

const OUTPUT_CHAR_LIMIT: usize = 500_000; // matches your Python SIZE_LIMIT behavior :contentReference[oaicite:3]{index=3}

#[derive(Parser, Debug)]
#[command(
    about = "Run a command and email completion status (Rust port of notify).",
    trailing_var_arg = true
)]
struct Args {
    /// Email address to notify
    #[arg(short = 'e', long = "email")]
    email: Option<String>,

    /// Send stdout/stderr in the email body (size-limited)
    #[arg(short = 'o', long = "send-output")]
    send_output: bool,

    /// Add or change an email address in the config file (interactive)
    #[arg(long = "add-email")]
    add_email: bool,

    /// Configure SMTP server settings interactively
    #[arg(long = "setup-server")]
    setup_server: bool,

    /// View the contents of the config file and exit
    #[arg(long = "view-config")]
    view_config: bool,

    /// Additional string to include in email subject
    #[arg(long = "ID")]
    id: Option<String>,

    /// Print the command that would be executed and exit
    #[arg(short = 'd', long = "dry-run")]
    dry_run: bool,

    /// Command to run (everything after --, or a single quoted string)
    #[arg(allow_hyphen_values = true)]
    command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ServerConfig {
    server: Option<String>,
    port: Option<u16>,
    from_address: Option<String>,
    /// Password can be literal or env var name (use password_env for clarity)
    password: Option<String>,
    /// Environment variable name containing the password
    password_env: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct User {
    name: String,
    email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Config {
    #[serde(default)]
    server: ServerConfig,
    #[serde(default)]
    users: Vec<User>,
}

impl Config {
    fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            // Create default config with comments
            let default_toml = r#"# notify configuration file
# Server settings for SMTP email delivery

[server]
# SMTP server address (e.g., smtp.gmail.com)
# server = "smtp.example.com"

# SMTP port (465 for implicit TLS, 587 for STARTTLS)
# port = 587

# Email address to send from
# from_address = "notify@example.com"

# Password: EITHER specify password_env (recommended) OR password (plaintext)
# password_env = "NOTIFY_PASSWORD"  # Read from environment variable
# password = "literal_password"      # NOT RECOMMENDED: stored in plaintext

# Example users (name/email pairs for quick selection)
# [[users]]
# name = "john"
# email = "john@example.com"
"#;
            fs::write(path, default_toml)?;
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        // Try to parse as TOML
        if let Ok(cfg) = toml::from_str(&content) {
            return Ok(cfg);
        }

        // Fall back to legacy format for backward compatibility
        eprintln!("notify: warning: legacy config format detected. Consider migrating to TOML format.");
        Self::load_legacy(path)
    }

    fn load_legacy(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let mut server = ServerConfig::default();
        let mut users = Vec::new();

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with("user") {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() == 3 {
                    users.push(User {
                        name: parts[1].to_string(),
                        email: parts[2].to_string(),
                    });
                }
                continue;
            }
            let mut it = line.split_whitespace();
            if let (Some(k), Some(v)) = (it.next(), it.next()) {
                match k {
                    "server" => server.server = Some(v.to_string()),
                    "port" => server.port = v.parse().ok(),
                    "from_address" => server.from_address = Some(v.to_string()),
                    "password" => server.password = Some(v.to_string()),
                    _ => {}
                }
            }
        }

        Ok(Self { server, users })
    }

    fn save(&self, path: &Path) -> Result<()> {
        let toml_str = toml::to_string_pretty(self)
            .context("Failed to serialize config to TOML")?;
        fs::write(path, toml_str)
            .with_context(|| format!("Failed to write config: {}", path.display()))?;
        Ok(())
    }

    fn upsert_user(&mut self, name: &str, email: &str) {
        if let Some(user) = self.users.iter_mut().find(|u| u.name == name) {
            user.email = email.to_string();
        } else {
            self.users.push(User {
                name: name.to_string(),
                email: email.to_string(),
            });
        }
    }

    fn get_password(&self) -> Result<String> {
        // Priority: password_env > password
        if let Some(env_var) = &self.server.password_env {
            return std::env::var(env_var)
                .with_context(|| format!("Environment variable '{}' not set", env_var));
        }
        
        self.server.password.clone()
            .ok_or_else(|| anyhow!("No password configured. Set 'password_env' or 'password' in config"))
    }

    fn require_server_config_interactive(&mut self, path: &Path) -> Result<()> {
        let mut missing = Vec::new();

        if self.server.server.is_none() {
            missing.push(("server", "SMTP server address (e.g., smtp.gmail.com)"));
        }
        if self.server.port.is_none() {
            missing.push(("port", "SMTP port (465 for SSL, 587 for STARTTLS)"));
        }
        if self.server.from_address.is_none() {
            missing.push(("from_address", "Email address to send from"));
        }
        if self.server.password.is_none() && self.server.password_env.is_none() {
            missing.push(("password", "Password or environment variable name"));
        }

        if missing.is_empty() {
            return Ok(());
        }

        eprintln!("Please provide the following server config info:");
        for (field, prompt) in &missing {
            match *field {
                "server" => {
                    let val = prompt_line(&format!("{prompt}: "))?;
                    self.server.server = Some(val);
                }
                "port" => {
                    let val = prompt_line(&format!("{prompt}: "))?;
                    self.server.port = Some(val.parse().context("Invalid port number")?);
                }
                "from_address" => {
                    let val = prompt_line(&format!("{prompt}: "))?;
                    self.server.from_address = Some(val);
                }
                "password" => {
                    eprintln!("Enter password (or environment variable name prefixed with '$'):");
                    let val = prompt_line("Password: ")?;
                    if let Some(env_name) = val.strip_prefix('$') {
                        self.server.password_env = Some(env_name.to_string());
                    } else {
                        eprintln!("notify: warning: storing password in plaintext. Consider using password_env instead.");
                        self.server.password = Some(val);
                    }
                }
                _ => {}
            }
        }

        let should_write = prompt_line("Write the provided information to config (y/n): ")?;
        if should_write.to_lowercase() == "y" {
            self.save(path)?;
            eprintln!("✓ Configuration saved to {}", path.display());
        }

        Ok(())
    }

    fn setup_server_interactive(&mut self, path: &Path) -> Result<()> {
        eprintln!("\n=== SMTP Server Configuration ===");
        eprintln!("Current settings:");
        eprintln!("  Server: {}", self.server.server.as_deref().unwrap_or("<not set>"));
        eprintln!("  Port: {}", self.server.port.map(|p| p.to_string()).unwrap_or_else(|| "<not set>".to_string()));
        eprintln!("  From: {}", self.server.from_address.as_deref().unwrap_or("<not set>"));
        
        if let Some(env_var) = &self.server.password_env {
            eprintln!("  Password: Using environment variable ${}", env_var);
        } else if self.server.password.is_some() {
            eprintln!("  Password: Set (plaintext in config)");
        } else {
            eprintln!("  Password: <not set>");
        }

        eprintln!("\nPress Enter to keep current value, or type new value:");
        
        // Server
        let val = prompt_line(&format!("SMTP server [{}]: ", 
            self.server.server.as_deref().unwrap_or("smtp.gmail.com")))?;
        if !val.is_empty() {
            self.server.server = Some(val);
        } else if self.server.server.is_none() {
            self.server.server = Some("smtp.gmail.com".to_string());
        }

        // Port
        let current_port = self.server.port.unwrap_or(587);
        let val = prompt_line(&format!("SMTP port (465 for SSL, 587 for STARTTLS) [{}]: ", current_port))?;
        if !val.is_empty() {
            self.server.port = Some(val.parse().context("Invalid port number")?);
        } else if self.server.port.is_none() {
            self.server.port = Some(current_port);
        }

        // From address
        let val = prompt_line(&format!("From email address [{}]: ",
            self.server.from_address.as_deref().unwrap_or("your-email@example.com")))?;
        if !val.is_empty() {
            self.server.from_address = Some(val);
        } else if self.server.from_address.is_none() {
            return Err(anyhow!("From address is required"));
        }

        // Password
        eprintln!("\nPassword configuration:");
        eprintln!("  1. Use environment variable (recommended)");
        eprintln!("  2. Store in config file (plaintext - not recommended)");
        eprintln!("  3. Keep current setting");
        let choice = prompt_line("Choose option [1/2/3]: ")?;
        
        match choice.as_str() {
            "1" => {
                let current = self.server.password_env.as_deref().unwrap_or("NOTIFY_PASSWORD");
                let val = prompt_line(&format!("Environment variable name [{}]: ", current))?;
                if !val.is_empty() {
                    self.server.password_env = Some(val);
                } else {
                    self.server.password_env = Some(current.to_string());
                }
                self.server.password = None; // Clear plaintext password
                eprintln!("notify: remember to set this variable: export {}='your-password'",
                    self.server.password_env.as_ref().unwrap());
            }
            "2" => {
                let val = prompt_line("Password: ")?;
                if !val.is_empty() {
                    self.server.password = Some(val);
                    self.server.password_env = None; // Clear env var
                    eprintln!("notify: warning: password stored in plaintext in config file.");
                }
            }
            "3" | "" => {
                // Keep current setting
            }
            _ => {
                eprintln!("notify: invalid choice, keeping current password setting.");
            }
        }

        eprintln!();
        self.save(path)?;
        eprintln!("✓ Server configuration saved to: {}", path.display());
        
        Ok(())
    }
}

fn prompt_line(msg: &str) -> Result<String> {
    eprint!("{msg}");
    io::stderr().flush().ok();
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

fn choose_email_interactive(cfg: &mut Config, path: &Path) -> Result<String> {
    if cfg.users.is_empty() {
        eprintln!("No users/emails found in config. Please add one now.");
        let name = prompt_line("Name to add to config: ")?;
        let email = prompt_line(&format!("Email address for user '{name}': "))?;
        cfg.upsert_user(&name, &email);
        cfg.save(path)?;
        return Ok(email);
    }

    loop {
        for (i, user) in cfg.users.iter().enumerate() {
            eprintln!("{}. {}", i + 1, user.name);
        }
        let choice = prompt_line("Select user number ('a' to add, 'q' to quit): ")?;
        if choice == "q" {
            return Err(anyhow!("Exiting at user request"));
        }
        if choice == "a" {
            let name = prompt_line("Name to add to config: ")?;
            let email = prompt_line(&format!("Email address for user '{name}': "))?;
            cfg.upsert_user(&name, &email);
            cfg.save(path)?;
            return Ok(email);
        }
        if let Ok(n) = choice.parse::<usize>() {
            if n >= 1 && n <= cfg.users.len() {
                return Ok(cfg.users[n - 1].email.clone());
            }
        }
        eprintln!("Input not understood.");
    }
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let days = secs / 86_400;
    let hrs = (secs % 86_400) / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;

    if days > 0 {
        format!("{days}d {hrs:02}:{mins:02}:{s:02}")
    } else {
        format!("{hrs:02}:{mins:02}:{s:02}")
    }
}

fn run_shell_command(cmd_string: &str, capture_output: bool) -> Result<(Output, String)> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    
    if capture_output {
        // Capture output for email while showing it in real-time
        let mut child = Command::new(&shell)
            .arg("-lc")
            .arg(cmd_string)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn shell for command: {cmd_string}"))?;
        
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        
        // Use threads to read stdout and stderr simultaneously
        // Only accumulate up to OUTPUT_CHAR_LIMIT to avoid memory issues with huge outputs
        let stdout_handle = thread::spawn(move || {
            let mut output = Vec::new();
            let mut total_chars = 0;
            let reader = io::BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(line) = line {
                    // Always print to CLI (no limits)
                    println!("{}", line);
                    
                    // Only accumulate for email if under limit
                    if total_chars < OUTPUT_CHAR_LIMIT {
                        let line_bytes = line.as_bytes();
                        let remaining = OUTPUT_CHAR_LIMIT.saturating_sub(total_chars);
                        let to_take = remaining.min(line_bytes.len());
                        output.extend_from_slice(&line_bytes[..to_take]);
                        output.push(b'\n');
                        total_chars += line_bytes.len() + 1;
                    }
                }
            }
            (output, total_chars)
        });
        
        let stderr_handle = thread::spawn(move || {
            let mut output = Vec::new();
            let mut total_chars = 0;
            let reader = io::BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(line) = line {
                    // Always print to CLI (no limits)
                    eprintln!("{}", line);
                    
                    // Only accumulate for email if under limit
                    if total_chars < OUTPUT_CHAR_LIMIT {
                        let line_bytes = line.as_bytes();
                        let remaining = OUTPUT_CHAR_LIMIT.saturating_sub(total_chars);
                        let to_take = remaining.min(line_bytes.len());
                        output.extend_from_slice(&line_bytes[..to_take]);
                        output.push(b'\n');
                        total_chars += line_bytes.len() + 1;
                    }
                }
            }
            (output, total_chars)
        });
        
        // Wait for both streams to complete
        let (stdout_output, stdout_chars) = stdout_handle.join().unwrap();
        let (stderr_output, stderr_chars) = stderr_handle.join().unwrap();
        
        let status = child.wait()?;
        
        // Combine stdout and stderr
        let mut combined = stdout_output;
        combined.extend_from_slice(&stderr_output);
        let total_chars = stdout_chars + stderr_chars;
        
        let mut output_str = String::from_utf8_lossy(&combined).to_string();
        if total_chars > OUTPUT_CHAR_LIMIT {
            output_str.push_str("\n... [output truncated for email]");
        }
        
        let output = Output {
            status,
            stdout: combined.clone(),
            stderr: Vec::new(),
        };
        
        Ok((output, output_str))
    } else {
        // Simple execution without capturing - just inherit stdio
        let status = Command::new(&shell)
            .arg("-lc")
            .arg(cmd_string)
            .status()
            .with_context(|| format!("Failed to spawn shell for command: {cmd_string}"))?;
        
        let output = Output {
            status,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        Ok((output, String::new()))
    }
}

fn build_email_subject(host: &str, id: Option<&str>, ref_name: &str) -> String {
    let tstring = Local::now().format("%m-%d-%y@%H:%M").to_string();
    let host_bracket = if host.is_empty() { "".to_string() } else { format!("[{host}]") };
    let id_part = id.map(|s| format!(" {s}")).unwrap_or_default();
    format!("{host_bracket}{id_part}: '{ref_name}' completed [{tstring}]")
}

fn send_email_tls(
    server: &str,
    port: u16,
    from_addr: &str,
    password: &str,
    to_addr: &str,
    subject: &str,
    plain_body: &str,
    html_body: &str,
) -> Result<()> {
    let from: Mailbox = from_addr.parse().context("Invalid from_address")?;
    let to: Mailbox = to_addr.parse().context("Invalid to_address")?;

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(subject)
        .multipart(
            MultiPart::alternative()
                .singlepart(SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(plain_body.to_string()))
                .singlepart(SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(html_body.to_string())),
        )?;

    let creds = Credentials::new(from_addr.to_string(), password.to_string());

    // Use TLS connection like Python's SMTP_SSL - try implicit TLS first (port 465), fall back to STARTTLS (587)
    let mailer = if port == 465 {
        SmtpTransport::relay(server)?
            .port(port)
            .credentials(creds)
            .build()
    } else {
        SmtpTransport::starttls_relay(server)?
            .port(port)
            .credentials(creds)
            .build()
    };

    // Retry with backoff matching Python's behavior
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 0..3 {
        match mailer.send(&email) {
            Ok(_) => return Ok(()),
            Err(e) => {
                last_err = Some(anyhow!("SMTP send failed: {}", e));
                if attempt < 2 {
                    eprintln!("notify: email send failed (attempt {}), retrying in 10s...", attempt + 1);
                    std::thread::sleep(Duration::from_secs(10));
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("Unknown email send error")))
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Use standard config directory (XDG on Linux, ~/Library/Application Support on macOS, etc.)
    let proj_dirs = ProjectDirs::from("", "", "notify")
        .ok_or_else(|| anyhow!("Could not determine config directory"))?;
    
    let config_dir = proj_dirs.config_dir();
    fs::create_dir_all(config_dir)?;
    
    let config_path = config_dir.join("config.toml");

    // Also check for legacy config in home directory for backward compatibility
    let legacy_config = std::env::var("HOME")
        .ok()
        .map(|h| std::path::PathBuf::from(h).join(".notify.config"))
        .filter(|p| p.exists());
    
    if let Some(legacy) = &legacy_config {
        if !config_path.exists() {
            eprintln!("notify: found legacy config at {}", legacy.display());
            eprintln!("notify: migrating to new location: {}", config_path.display());
            fs::copy(legacy, &config_path)?;
        }
    }

    let mut cfg = Config::load(&config_path)?;

    if args.view_config {
        let mut s = String::new();
        fs::File::open(&config_path)?.read_to_string(&mut s)?;
        print!("{s}");
        return Ok(());
    }

    if args.setup_server {
        cfg.setup_server_interactive(&config_path)?;
        return Ok(());
    }

    if args.add_email {
        let name = prompt_line("Name to add to config: ")?;
        let email = prompt_line(&format!("Email address for user '{name}': "))?;
        cfg.upsert_user(&name, &email);
        cfg.save(&config_path)?;
        eprintln!("✓ '{}' added to {}", name, config_path.display());
        return Ok(());
    }

    if args.command.is_empty() {
        return Err(anyhow!("No command provided. Usage: notify [OPTIONS] -- <command>"));
    }

    // Support both syntaxes: notify "cmd" -o OR notify -o -- cmd
    let cmd_string = if args.command.len() == 1 && args.command[0].contains(' ') {
        // Single quoted string: notify "ls -lhtr" -o
        args.command[0].clone()
    } else {
        // Multiple args with --: notify -o -- ls -lhtr
        args.command.join(" ")
    };
    
    if args.dry_run {
        println!("{cmd_string}");
        return Ok(());
    }

    //

    // Choose target email
    let target_email = match args.email {
        Some(e) => e,
        None => choose_email_interactive(&mut cfg, &config_path)?,
    };

    if !target_email.contains('@') {
        return Err(anyhow!("Email address missing '@' symbol. Exiting."));
    }

    // Ensure server config exists (interactive prompt if missing)
    cfg.require_server_config_interactive(&config_path)?;

    let server = cfg.server.server.as_ref().ok_or_else(|| anyhow!("Missing 'server' in config"))?;
    let from_address = cfg.server.from_address.as_ref().ok_or_else(|| anyhow!("Missing 'from_address' in config"))?;
    let password = cfg.get_password()?;
    let port = cfg.server.port.ok_or_else(|| anyhow!("Missing 'port' in config"))?;

    // "REF_NAME" is first token of the command
    let ref_name = cmd_string.split_whitespace().next().unwrap_or("command").to_string();

    let run_dir = std::env::current_dir()?.display().to_string();
    let host = get_descriptive_hostname();

    let start = Instant::now();
    let (out, captured_output) = run_shell_command(&cmd_string, args.send_output)?;
    let runtime = format_duration(start.elapsed());

    let status_code = out.status.code().unwrap_or(-1);

    // Use captured output if available, otherwise combine stdout/stderr
    let output_text = if args.send_output {
        captured_output
    } else {
        let combined = [out.stdout.as_slice(), out.stderr.as_slice()].concat();
        String::from_utf8_lossy(&combined).to_string()
    };

    let subject = build_email_subject(host.as_str(), args.id.as_deref(), &ref_name);

    let mut plain_lines = vec![
        format!("Arguments: {cmd_string}"),
        format!("Runtime: {runtime}"),
        format!("Return value: {status_code}"),
        format!("Location: {run_dir}"),
    ];

    let mut html_lines = vec![
        format!("<b>Arguments</b>: {}", html_escape(&cmd_string)),
        format!("<b>Runtime</b>: {runtime}"),
        format!("<b>Return value</b>: {status_code}"),
        format!("<b>Location</b>: {}", html_escape(&run_dir)),
    ];

    if args.send_output {
        plain_lines.push("".to_string());
        plain_lines.push("Output:".to_string());
        plain_lines.push(output_text.clone());

        html_lines.push(format!("<b>Output:</b><br /><pre>{}</pre>", html_escape(&output_text)));
    }

    let plain_body = plain_lines.join("\n");
    let html_body = format!("<pre>{}</pre>", html_lines.join("<br />\n"));

    send_email_tls(
        server,
        port,
        from_address,
        password.as_str(),
        &&target_email,
        &subject,
        &plain_body,
        &html_body,
    )?;

    eprintln!("notify: command completed in {runtime}");
    
    // Exit with the same code as the wrapped command
    std::process::exit(status_code);
}

fn get_descriptive_hostname() -> String {
    if let Ok(name) = whoami::hostname() {
        // On macOS, gethostname() often returns something generic like "Mac"
        // Fall back to LocalHostName (Bonjour name) which is more specific
        #[cfg(target_os = "macos")]
        if name.len() <= 4 || name.eq_ignore_ascii_case("mac") || name.eq_ignore_ascii_case("macbook") {
            if let Ok(output) = Command::new("scutil")
                .args(["--get", "LocalHostName"])
                .output()
            {
                let local = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !local.is_empty() {
                    return local;
                }
            }
        }
        return name;
    }
    "unknown".to_string()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
