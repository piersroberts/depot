//! Depot - Portable File Sharing Server
//!
//! A single-binary file server supporting FTP and HTTP protocols,
//! designed for compatibility with both modern and retro systems.
//!
//! # Features
//! - FTP server with anonymous access and user authentication
//! - HTTP server with retro-compatible directory listings
//! - Virtual filesystem merging multiple directories
//! - Web-based administration panel
//! - TOML configuration file
//!
//! # Usage
//! ```
//! depot                       # Run with default or depot.toml config
//! depot -c config.toml        # Run with specific config file
//! depot --init                # Generate example config file
//! ```

mod admin;
mod config;
mod protocols;
mod random_creds;
mod templates;
mod themes;
mod vfs;

use config::Config;
use protocols::ftp::FtpServer;
use protocols::http::HttpServer;
use protocols::smb::SmbServer;
use protocols::ProtocolServer;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing_subscriber::EnvFilter;

/// Command-line arguments
struct Args {
    config_path: Option<PathBuf>,
    init_config: bool,
    help: bool,
    version: bool,
    /// User management: add-user <username>
    add_user: Option<String>,
    /// User management: remove-user <username>
    remove_user: Option<String>,
    /// User management: set-password <username>
    set_password: Option<String>,
    /// User management: grant <username> <share>
    grant: Option<(String, String)>,
    /// User management: revoke <username> <share>
    revoke: Option<(String, String)>,
    /// List all users
    list_users: bool,
}

impl Args {
    fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let mut result = Self {
            config_path: None,
            init_config: false,
            help: false,
            version: false,
            add_user: None,
            remove_user: None,
            set_password: None,
            grant: None,
            revoke: None,
            list_users: false,
        };

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "-c" | "--config" => {
                    if i + 1 < args.len() {
                        result.config_path = Some(PathBuf::from(&args[i + 1]));
                        i += 1;
                    }
                }
                "--init" => result.init_config = true,
                "-h" | "--help" => result.help = true,
                "-v" | "--version" => result.version = true,
                "add-user" => {
                    if i + 1 < args.len() {
                        result.add_user = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "remove-user" => {
                    if i + 1 < args.len() {
                        result.remove_user = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "set-password" => {
                    if i + 1 < args.len() {
                        result.set_password = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "grant" => {
                    if i + 2 < args.len() {
                        result.grant = Some((args[i + 1].clone(), args[i + 2].clone()));
                        i += 2;
                    }
                }
                "revoke" => {
                    if i + 2 < args.len() {
                        result.revoke = Some((args[i + 1].clone(), args[i + 2].clone()));
                        i += 2;
                    }
                }
                "list-users" => result.list_users = true,
                _ => {}
            }
            i += 1;
        }

        result
    }
}

fn print_help() {
    println!(
        r#"Depot - Portable File Sharing Server

USAGE:
    depot [OPTIONS]
    depot [COMMAND]

OPTIONS:
    -c, --config <FILE>    Path to configuration file (default: depot.toml)
    --init                 Generate example configuration file
    -h, --help             Print this help message
    -v, --version          Print version information

USER MANAGEMENT:
    add-user <username>           Add a new user (prompts for password)
    remove-user <username>        Remove a user
    set-password <username>       Change a user's password
    grant <username> <share>      Grant user access to a share
    revoke <username> <share>     Revoke user access to a share
    list-users                    List all configured users

EXAMPLES:
    depot                         Run with depot.toml in current directory
    depot -c myconfig.toml        Run with specific config file
    depot --init                  Create example depot.toml file
    depot add-user alice          Add user 'alice' to the config
    depot grant alice Public      Grant 'alice' access to 'Public' share

For more information, visit: https://github.com/your-repo/depot"#
    );
}

fn print_version() {
    println!("Depot {}", env!("CARGO_PKG_VERSION"));
}

fn init_config() -> anyhow::Result<()> {
    let config_path = Config::default_config_path();

    if config_path.exists() {
        anyhow::bail!(
            "Configuration file already exists: {}",
            config_path.display()
        );
    }

    // Generate random admin credentials
    let admin_username = random_creds::generate_username();
    let admin_password = random_creds::generate_password(16);

    let mut shares = std::collections::HashMap::new();
    shares.insert(
        "Public".to_string(),
        config::ShareConfig {
            path: PathBuf::from("/path/to/public/files"),
            virtual_path: "/public".to_string(),
            read_only: true,
            description: Some("Public files accessible to everyone".to_string()),
            enabled: true,
        },
    );
    shares.insert(
        "Games".to_string(),
        config::ShareConfig {
            path: PathBuf::from("/path/to/games"),
            virtual_path: "/games".to_string(),
            read_only: true,
            description: Some("Retro game collection".to_string()),
            enabled: true,
        },
    );

    let example_config = Config {
        server_name: "Depot".to_string(),
        shares,
        users: std::collections::HashMap::new(),
        protocols: config::ProtocolsConfig {
            ftp: config::FtpConfig {
                enabled: true,
                port: 2121,
                anonymous: true,
                ..Default::default()
            },
            http: config::HttpConfig {
                enabled: true,
                port: 8080,
                retro_compatible: false,
                theme: "modern".to_string(),
                footer_message: None,
                ..Default::default()
            },
            smb: Default::default(),
        },
        admin: config::AdminConfig {
            enabled: true,
            port: 8888,
            username: admin_username.clone(),
            password: admin_password.clone(),
            ..Default::default()
        },
        log_level: "info".to_string(),
    };

    example_config.save(&config_path)?;

    println!("Created example configuration: {}", config_path.display());
    println!();
    println!("Admin panel credentials (save these!):");
    println!("  Username: {admin_username}");
    println!("  Password: {admin_password}");
    println!();
    println!("Please edit the file to configure your shares before starting the server.");

    Ok(())
}

/// Prompt for password input (hides input on supported terminals)
fn prompt_password(prompt: &str) -> anyhow::Result<String> {
    use std::io::{self, Write};

    print!("{prompt}");
    io::stdout().flush()?;

    // Use rpassword for cross-platform hidden input
    let password = rpassword::read_password().unwrap_or_else(|_| {
        // Fallback to regular input
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        input.trim().to_string()
    });

    Ok(password)
}

/// Add a new user to the configuration
fn cmd_add_user(config_path: &PathBuf, username: &str) -> anyhow::Result<()> {
    if !config_path.exists() {
        anyhow::bail!(
            "Configuration file not found: {}\nRun 'depot --init' first.",
            config_path.display()
        );
    }

    let mut config = Config::load(config_path)?;

    // Check if user already exists
    if config.users.contains_key(username) {
        anyhow::bail!("User '{username}' already exists");
    }

    // Prompt for password
    let password = prompt_password("Enter password: ")?;
    if password.is_empty() {
        anyhow::bail!("Password cannot be empty");
    }

    let password_confirm = prompt_password("Confirm password: ")?;
    if password != password_confirm {
        anyhow::bail!("Passwords do not match");
    }

    // Create user with no share access by default
    let user = config::User::new(&password, Vec::new())?;
    config.add_user(username.to_string(), user)?;
    config.save(config_path)?;

    println!("User '{username}' added successfully.");
    println!("Use 'depot grant {username} <share>' to grant access to shares.");

    Ok(())
}

/// Remove a user from the configuration
fn cmd_remove_user(config_path: &PathBuf, username: &str) -> anyhow::Result<()> {
    if !config_path.exists() {
        anyhow::bail!("Configuration file not found: {}", config_path.display());
    }

    let mut config = Config::load(config_path)?;
    config.remove_user(username)?;
    config.save(config_path)?;

    println!("User '{username}' removed.");

    Ok(())
}

/// Change a user's password
fn cmd_set_password(config_path: &PathBuf, username: &str) -> anyhow::Result<()> {
    if !config_path.exists() {
        anyhow::bail!("Configuration file not found: {}", config_path.display());
    }

    let mut config = Config::load(config_path)?;

    // Check if user exists
    if !config.users.contains_key(username) {
        anyhow::bail!("User '{username}' not found");
    }

    // Prompt for new password
    let password = prompt_password("Enter new password: ")?;
    if password.is_empty() {
        anyhow::bail!("Password cannot be empty");
    }

    let password_confirm = prompt_password("Confirm new password: ")?;
    if password != password_confirm {
        anyhow::bail!("Passwords do not match");
    }

    // Update password hash
    let new_hash = config::hash_password(&password)?;
    if let Some(user) = config.users.get_mut(username) {
        user.password_hash = new_hash;
    }
    config.save(config_path)?;

    println!("Password updated for user '{username}'.");

    Ok(())
}

/// Grant a user access to a share
fn cmd_grant(config_path: &PathBuf, username: &str, share_name: &str) -> anyhow::Result<()> {
    if !config_path.exists() {
        anyhow::bail!("Configuration file not found: {}", config_path.display());
    }

    let mut config = Config::load(config_path)?;

    // Check share exists (unless granting "*" for all)
    if share_name != "*" && !config.shares.contains_key(share_name) {
        let available: Vec<_> = config.shares.keys().map(|s| s.as_str()).collect();
        anyhow::bail!(
            "Share '{}' not found. Available shares: {}",
            share_name,
            available.join(", ")
        );
    }

    config.grant_share(username, share_name)?;
    config.save(config_path)?;

    if share_name == "*" {
        println!("User '{username}' granted access to ALL shares.");
    } else {
        println!("User '{username}' granted access to '{share_name}'.");
    }

    Ok(())
}

/// Revoke a user's access to a share
fn cmd_revoke(config_path: &PathBuf, username: &str, share_name: &str) -> anyhow::Result<()> {
    if !config_path.exists() {
        anyhow::bail!("Configuration file not found: {}", config_path.display());
    }

    let mut config = Config::load(config_path)?;
    config.revoke_share(username, share_name)?;
    config.save(config_path)?;

    println!("User '{username}' access to '{share_name}' revoked.");

    Ok(())
}

/// List all configured users
fn cmd_list_users(config_path: &PathBuf) -> anyhow::Result<()> {
    if !config_path.exists() {
        anyhow::bail!("Configuration file not found: {}", config_path.display());
    }

    let config = Config::load(config_path)?;

    if config.users.is_empty() {
        println!("No users configured.");
        println!("Use 'depot add-user <username>' to add a user.");
        return Ok(());
    }

    println!("Configured users:");
    println!("{:-<60}", "");
    for (username, user) in &config.users {
        let status = if user.enabled { "enabled" } else { "DISABLED" };
        let shares = if user.shares.is_empty() {
            "none".to_string()
        } else {
            user.shares.join(", ")
        };
        println!("  {username} ({status})");
        println!("    Shares: {shares}");
        if let Some(desc) = &user.description {
            println!("    Description: {desc}");
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.help {
        print_help();
        return Ok(());
    }

    if args.version {
        print_version();
        return Ok(());
    }

    if args.init_config {
        return init_config();
    }

    // Determine config path for user management commands
    let config_path = args
        .config_path
        .clone()
        .unwrap_or_else(Config::default_config_path);

    // Handle user management commands
    if let Some(username) = &args.add_user {
        return cmd_add_user(&config_path, username);
    }

    if let Some(username) = &args.remove_user {
        return cmd_remove_user(&config_path, username);
    }

    if let Some(username) = &args.set_password {
        return cmd_set_password(&config_path, username);
    }

    if let Some((username, share)) = &args.grant {
        return cmd_grant(&config_path, username, share);
    }

    if let Some((username, share)) = &args.revoke {
        return cmd_revoke(&config_path, username, share);
    }

    if args.list_users {
        return cmd_list_users(&config_path);
    }

    // Load configuration for server startup
    let config = if config_path.exists() {
        Config::load(&config_path)?
    } else {
        println!("No configuration file found. Using defaults.");
        println!("Run 'depot --init' to create an example configuration.\n");
        Config::default()
    };

    // Initialize logging
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    tracing::info!("Depot File Server v{}", env!("CARGO_PKG_VERSION"));
    tracing::info!("Server name: {}", config.server_name);

    // Initialize templates
    templates::init();

    // Create virtual filesystem
    let vfs = vfs::create_vfs(&config.shares);
    tracing::info!(
        "Loaded {} share(s)",
        config.shares.iter().filter(|(_, s)| s.enabled).count()
    );

    // Wrap config in Arc<RwLock> for admin panel
    let config = Arc::new(RwLock::new(config));

    // Start FTP server
    let ftp_server = {
        let cfg = config.read().unwrap();
        FtpServer::new(cfg.protocols.ftp.clone(), cfg.users.clone(), vfs.clone())
    };
    ftp_server.start().await?;

    // Start HTTP server
    let http_server = {
        let cfg = config.read().unwrap();
        HttpServer::new(cfg.protocols.http.clone(), cfg.users.clone(), vfs.clone())
    };
    http_server.start().await?;

    // Start SMB server
    let smb_server = {
        let cfg = config.read().unwrap();
        SmbServer::new(
            cfg.protocols.smb.clone(),
            vfs.clone(),
            cfg.server_name.clone(),
        )
    };
    smb_server.start().await?;

    // Start admin panel
    let admin_server = {
        let cfg = config.read().unwrap();
        admin::AdminServer::new(cfg.admin.clone(), config.clone(), vfs.clone())
    };
    admin_server.start().await?;

    // Print access information
    {
        let cfg = config.read().unwrap();
        println!("\n═══════════════════════════════════════════════════════════");
        println!("  Depot File Server is running!");
        println!("═══════════════════════════════════════════════════════════");

        if cfg.protocols.ftp.enabled {
            println!(
                "  FTP:   ftp://{}:{}",
                get_local_ip().unwrap_or_else(|| cfg.protocols.ftp.bind_address.to_string()),
                cfg.protocols.ftp.port
            );
        }

        if cfg.protocols.http.enabled {
            println!(
                "  HTTP:  http://{}:{}",
                get_local_ip().unwrap_or_else(|| cfg.protocols.http.bind_address.to_string()),
                cfg.protocols.http.port
            );
        }

        if cfg.protocols.smb.enabled {
            let ip = get_local_ip().unwrap_or_else(|| cfg.protocols.smb.bind_address.to_string());
            println!(
                "  SMB:   \\\\{}\\<share>  (port {})",
                ip, cfg.protocols.smb.port
            );
        }

        if cfg.admin.enabled {
            println!(
                "  Admin: http://{}:{}",
                cfg.admin.bind_address, cfg.admin.port
            );
        }

        println!("═══════════════════════════════════════════════════════════");
        println!("  Press Ctrl+C to stop the server");
        println!("═══════════════════════════════════════════════════════════\n");
    }

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;

    tracing::info!("Shutting down...");

    // Stop servers
    ftp_server.stop().await?;
    http_server.stop().await?;
    admin_server.stop().await?;

    tracing::info!("Goodbye!");

    Ok(())
}

/// Try to get local IP address for display purposes
fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;

    // This doesn't actually send anything, just uses the OS routing to find local IP
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}
