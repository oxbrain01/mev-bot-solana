use crate::chain::pools::{MintPoolData, PumpPool, RaydiumPool};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use std::{collections::HashMap, sync::Arc, time::Instant};
use tracing::{info, warn};

/// Token price information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPrice {
    pub mint: String,
    pub price_usd: f64,
    pub price_sol: f64,
    pub volume_24h: f64,
    pub market_cap: f64,
    pub timestamp: u64,
    pub source: String,
}

/// Price comparison for arbitrage opportunities
#[derive(Debug, Clone)]
pub struct PriceComparison {
    pub token_mint: String,
    pub dex_prices: HashMap<String, f64>, // DEX name -> price in SOL
    pub best_buy_price: f64,
    pub best_sell_price: f64,
    pub best_buy_dex: String,
    pub best_sell_dex: String,
    pub price_spread: f64,
    pub potential_profit_percent: f64,
    pub timestamp: Instant,
}

/// Market data fetcher
pub struct MarketDataFetcher {
    rpc_client: Arc<RpcClient>,
    price_cache: HashMap<String, TokenPrice>,
    cache_ttl_seconds: u64,
}

impl MarketDataFetcher {
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            price_cache: HashMap::new(),
            cache_ttl_seconds: 30, // 30 seconds cache
        }
    }

    /// Fetch token price from multiple sources
    pub async fn fetch_token_price(&mut self, mint: &str) -> Result<TokenPrice> {
        // Check cache first
        if let Some(cached_price) = self.price_cache.get(mint) {
            if cached_price.timestamp + self.cache_ttl_seconds > 
               std::time::SystemTime::now()
                   .duration_since(std::time::UNIX_EPOCH)
                   .unwrap()
                   .as_secs() {
                return Ok(cached_price.clone());
            }
        }

        println!("Fetching price for mint: {}", mint);
        println!("-------\n");

        // Try multiple price sources
        let mut price_sources = Vec::new();

        // Try Jupiter API
        if let Ok(price) = self.fetch_jupiter_price(mint).await {
            price_sources.push(price);
        }

        // println!("Price sources after Jupiter: {:?}", price_sources);
        //         println!("-------\n");

        // // Try Birdeye API
        // if let Ok(price) = self.fetch_birdeye_price(mint).await {
        //     price_sources.push(price);
        // }

        // println!("Price sources after Birdeye: {:?}", price_sources);
        //         println!("-------\n");

        // Try CoinGecko API
        // if let Ok(price) = self.fetch_coingecko_price(mint).await {
        //     price_sources.push(price);
        // }

        // println!("Price sources after CoinGecko: {:?}", price_sources);
        //         println!("-------\n");

        // if price_sources.is_empty() {
        //     return Err(anyhow!("Failed to fetch price from any source for mint: {}", mint));
        // }

        // println!("Price sources after all sources: {:?}", price_sources);
        //         println!("-------\n");

        // Use the most recent price or average of recent prices
        let best_price = self.select_best_price(price_sources);

        // println!("Best price: {:?}", best_price);
        //         println!("-------\n");
        // Cache the result
        self.price_cache.insert(mint.to_string(), best_price.clone());
        
        Ok(best_price)
    }

    /// Fetch price from Jupiter API
    async fn fetch_jupiter_price(&self, mint: &str) -> Result<TokenPrice> {
        let url = format!(
            "https://api.jup.ag/price/v3?ids={}",
            mint
        );

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("x-api-key", "acfcdaea-caf7-4ff2-800c-3feb23e697ac")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("Jupiter API request failed with status: {}", response.status()));
        }

        let data: serde_json::Value = response.json().await?;

        // According to API docs, response is directly an object with token ID as key
        if let Some(price_data) = data.get(mint) {
            let price_usd = price_data
                .get("usdPrice")
                .and_then(|p| p.as_f64())
                .ok_or_else(|| anyhow!("Invalid price format: missing usdPrice"))?;

            // Fetch SOL price to calculate price_sol accurately
            let sol_price_usd = self.get_sol_price_usd().await.unwrap_or(150.0); // Default fallback
            let price_sol = price_usd / sol_price_usd;

            Ok(TokenPrice {
                mint: mint.to_string(),
                price_usd,
                price_sol,
                volume_24h: 0.0, // Jupiter doesn't provide volume in this endpoint
                market_cap: 0.0, // Jupiter doesn't provide market cap in this endpoint
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                source: "jupiter".to_string(),
            })
        } else {
            Err(anyhow!("Price not found in Jupiter response for mint: {}", mint))
        }
    }

    /// Helper method to fetch SOL price in USD
    async fn get_sol_price_usd(&self) -> Result<f64> {
        let sol_mint = "So11111111111111111111111111111111111111112";
        let url = format!(
            "https://api.jup.ag/price/v3?ids={}",
            sol_mint
        );

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("x-api-key", "acfcdaea-caf7-4ff2-800c-3feb23e697ac")
            .send()
            .await?;

        if response.status().is_success() {
            let data: serde_json::Value = response.json().await?;
            if let Some(sol_data) = data.get(sol_mint) {
                if let Some(price) = sol_data.get("usdPrice").and_then(|p| p.as_f64()) {
                    return Ok(price);
                }
            }
        }
        
        Err(anyhow!("Failed to fetch SOL price"))
    }

    /// Fetch price from Birdeye API
    async fn fetch_birdeye_price(&self, mint: &str) -> Result<TokenPrice> {
        let url = format!(
            "https://public-api.birdeye.so/public/price?address={}",
            mint
        );

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("X-API-KEY", "63535f47553c4e419438b9ada648abe9") // You'll need to get an API key
            .send()
            .await?;

        if response.status().is_success() {
            let data: serde_json::Value = response.json().await?;
            
            if let Some(price_data) = data.get("data") {
                let price_usd = price_data
                    .get("value")
                    .and_then(|p| p.as_f64())
                    .ok_or_else(|| anyhow!("Invalid price format"))?;

                let volume_24h = price_data
                    .get("volume24h")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                let market_cap = price_data
                    .get("marketCap")
                    .and_then(|m| m.as_f64())
                    .unwrap_or(0.0);

                Ok(TokenPrice {
                    mint: mint.to_string(),
                    price_usd,
                    price_sol: price_usd / 100.0, // Rough conversion
                    volume_24h,
                    market_cap,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    source: "birdeye".to_string(),
                })
            } else {
                Err(anyhow!("Invalid Birdeye response format"))
            }
        } else {
            Err(anyhow!("Birdeye API request failed"))
        }
    }

    /// Fetch price from CoinGecko API
    async fn fetch_coingecko_price(&self, mint: &str) -> Result<TokenPrice> {
        // Note: CoinGecko requires coin IDs, not mint addresses
        // This is a simplified implementation
        let url = format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd,sol&include_24hr_vol=true&include_market_cap=true",
            mint
        );

        let response = reqwest::get(&url).await?;
        let data: serde_json::Value = response.json().await?;

        if let Some(coin_data) = data.get(mint) {
            let price_usd = coin_data
                .get("usd")
                .and_then(|p| p.as_f64())
                .ok_or_else(|| anyhow!("Invalid USD price"))?;

            let price_sol = coin_data
                .get("sol")
                .and_then(|p| p.as_f64())
                .unwrap_or(price_usd / 100.0);

            let volume_24h = coin_data
                .get("usd_24h_vol")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            let market_cap = coin_data
                .get("usd_market_cap")
                .and_then(|m| m.as_f64())
                .unwrap_or(0.0);

            Ok(TokenPrice {
                mint: mint.to_string(),
                price_usd,
                price_sol,
                volume_24h,
                market_cap,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                source: "coingecko".to_string(),
            })
        } else {
            Err(anyhow!("Coin not found in CoinGecko response"))
        }
    }

    /// Select the best price from multiple sources
    fn select_best_price(&self, prices: Vec<TokenPrice>) -> TokenPrice {
        // For now, just return the first price
        // In a real implementation, you might want to:
        // - Average the prices
        // - Weight by source reliability
        // - Filter out outliers
        prices.into_iter().next().unwrap()
    }

    /// Calculate arbitrage opportunities from pool data
    pub async fn calculate_arbitrage_opportunities(
        &self,
        pool_data: &MintPoolData,
    ) -> Result<Vec<PriceComparison>> {
        let mut opportunities = Vec::new();
        let token_mint = pool_data.mint.to_string();

        // Calculate prices from different DEX pools
        let mut dex_prices = HashMap::new();

        // Calculate Raydium prices
        for pool in &pool_data.raydium_pools {
            if let Ok(price) = self.calculate_raydium_price(pool).await {
                dex_prices.insert("raydium".to_string(), price);
            }
        }

        // Calculate Pump prices
        for pool in &pool_data.pump_pools {
            if let Ok(price) = self.calculate_pump_price(pool).await {
                dex_prices.insert("pump".to_string(), price);
            }
        }

        println!("dex_prices: {:?}", dex_prices);
        println!("-------\n");
        if dex_prices.len() >= 2 {
            // Find best buy and sell prices
            let (best_buy_dex, best_buy_price) = dex_prices
                .iter()
                .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .unwrap();

            let (best_sell_dex, best_sell_price) = dex_prices
                .iter()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .unwrap();  

            let price_spread = best_sell_price - best_buy_price;
            let potential_profit_percent = (price_spread / best_buy_price) * 100.0;
            println!("potential_profit_percent: {:?}", potential_profit_percent);

            // Only consider opportunities with significant spread
            if potential_profit_percent > 0.5 {
                opportunities.push(PriceComparison {
                    token_mint: token_mint.clone(),
                    dex_prices: dex_prices.clone(),
                    best_buy_price: *best_buy_price,
                    best_sell_price: *best_sell_price,
                    best_buy_dex: best_buy_dex.clone(),
                    best_sell_dex: best_sell_dex.clone(),
                    price_spread,
                    potential_profit_percent,
                    timestamp: Instant::now(),
                });
            }
        }

        Ok(opportunities)
    }

    /// Calculate price from Raydium pool
    async fn calculate_raydium_price(&self, pool: &RaydiumPool) -> Result<f64> {
        // Fetch token account balances using RPC client (handles parsing automatically)
        let token_balance = self.rpc_client
            .get_token_account_balance(&pool.token_vault)
            .map_err(|e| anyhow!("Failed to fetch token vault balance: {}", e))?;
        
        let sol_balance = self.rpc_client
            .get_token_account_balance(&pool.sol_vault)
            .map_err(|e| anyhow!("Failed to fetch SOL vault balance: {}", e))?;

        // Parse amounts from UI strings (e.g., "1000.5" -> 1000.5)
        let token_amount: f64 = token_balance.amount.parse::<u64>()
            .map_err(|e| anyhow!("Failed to parse token balance: {}", e))?
            as f64 / 10_f64.powi(token_balance.decimals as i32);
        
        let sol_amount: f64 = sol_balance.amount.parse::<u64>()
            .map_err(|e| anyhow!("Failed to parse SOL balance: {}", e))?
            as f64 / 10_f64.powi(sol_balance.decimals as i32);

        if token_amount == 0.0 {
            return Err(anyhow!("Token reserve is zero, cannot calculate price"));
        }

        // Calculate price: price = sol_amount / token_amount
        // This gives us the price of 1 token in SOL
        let price = sol_amount / token_amount;

        Ok(price)
    }

    /// Calculate price from Pump pool
    async fn calculate_pump_price(&self, pool: &PumpPool) -> Result<f64> {
        // Fetch token account balances using RPC client (handles parsing automatically)
        let token_balance = self.rpc_client
            .get_token_account_balance(&pool.token_vault)
            .map_err(|e| anyhow!("Failed to fetch token vault balance: {}", e))?;
        
        let sol_balance = self.rpc_client
            .get_token_account_balance(&pool.sol_vault)
            .map_err(|e| anyhow!("Failed to fetch SOL vault balance: {}", e))?;

        // Parse amounts from UI strings (e.g., "1000.5" -> 1000.5)
        let token_amount: f64 = token_balance.amount.parse::<u64>()
            .map_err(|e| anyhow!("Failed to parse token balance: {}", e))?
            as f64 / 10_f64.powi(token_balance.decimals as i32);
        
        let sol_amount: f64 = sol_balance.amount.parse::<u64>()
            .map_err(|e| anyhow!("Failed to parse SOL balance: {}", e))?
            as f64 / 10_f64.powi(sol_balance.decimals as i32);

        if token_amount == 0.0 {
            return Err(anyhow!("Token reserve is zero, cannot calculate price"));
        }

        // Calculate price: price = sol_amount / token_amount
        // This gives us the price of 1 token in SOL
        let price = sol_amount / token_amount;

        Ok(price)
    }

    /// Get market statistics
    pub fn get_market_stats(&self) -> HashMap<String, usize> {
        let mut stats = HashMap::new();
        stats.insert("cached_prices".to_string(), self.price_cache.len());
        stats
    }

    /// Clear expired cache entries
    pub fn clear_expired_cache(&mut self) {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.price_cache.retain(|_, price| {
            current_time - price.timestamp < self.cache_ttl_seconds
        });
    }
}

/// Real-time price monitor
pub struct PriceMonitor {
    market_fetcher: MarketDataFetcher,
    monitoring_interval_ms: u64,
    price_threshold: f64,
}

impl PriceMonitor {
    pub fn new(
        rpc_client: Arc<RpcClient>,
        monitoring_interval_ms: u64,
        price_threshold: f64,
    ) -> Self {
        Self {
            market_fetcher: MarketDataFetcher::new(rpc_client),
            monitoring_interval_ms,
            price_threshold,
        }
    }

    /// Start monitoring prices for arbitrage opportunities
    pub async fn start_monitoring(&mut self, mints: Vec<String>) {
        info!("Starting price monitoring for {} tokens", mints.len());

        loop {
            for mint in &mints {
                match self.market_fetcher.fetch_token_price(mint).await {
                    Ok(price) => {
                        info!(
                            "Token {}: ${:.6} USD, {:.6} SOL (source: {})",
                            mint, price.price_usd, price.price_sol, price.source
                        );
                    }
                    Err(e) => {
                        warn!("Failed to fetch price for {}: {}", mint, e);
                    }
                }
            }

            // Clear expired cache
            self.market_fetcher.clear_expired_cache();

            // Wait before next monitoring cycle
            tokio::time::sleep(tokio::time::Duration::from_millis(
                self.monitoring_interval_ms,
            ))
            .await;
        }
    }
}

