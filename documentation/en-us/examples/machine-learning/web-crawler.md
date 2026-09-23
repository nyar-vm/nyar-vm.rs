# 网络爬虫

Valkyrie 提供了强大的网络爬虫框架，支持异步并发、智能调度、数据提取和存储等功能。通过类型安全的 API 和零成本抽象，为大规模数据采集提供高效解决方案。

## 核心特性

### 异步 HTTP 客户端

```valkyrie
use std::http::{HttpClient, Request, Response, Method}
use std::async::{Future, Stream}
use std::time::Duration

# HTTP 客户端配置
class CrawlerConfig {
    max_concurrent: usize      # 最大并发数 ∥
    request_delay: Duration    # 请求间隔 Δt
    timeout: Duration          # 超时时间 τ
    user_agent: string         # 用户代理
    max_retries: u32           # 最大重试次数 ℝ
}

# 异步 HTTP 客户端
class AsyncHttpClient {
    config: CrawlerConfig,
    client: HttpClient
    
    async micro new(config: CrawlerConfig) -> AsyncHttpClient {
        let client = HttpClient::builder()
            .timeout(config.timeout)
            .user_agent(config.user_agent.clone())
            .build()
            
        AsyncHttpClient { config, client }
    }
    
    async micro get(self, url: string) -> Result<Response, CrawlerError> {
        let request = Request::builder()
            .method(Method::GET)
            .uri(url)
            .build()?
            
        let mut ℝ = 0  # 重试计数器
        
        loop {
            match self.client.send(request.clone()).await {
                Fine { value: response } => {
                    if response.status().is_success() {
                        return Ok(response)  # 成功响应 ✓
                    } else if response.status().is_server_error() && ℝ < self.config.max_retries {
                        ℝ += 1
                        tokio::time::sleep(Duration::from_secs(2_u64.pow(ℝ))).await  # 指数退避 2^ℝ
                        continue
                    } else {
                        return Err(CrawlerError::HttpError(response.status()))  # HTTP错误 ✗
                    }
                },
                Fail { error: e } if ℝ < self.config.max_retries => {
                    ℝ += 1
                    tokio::time::sleep(Duration::from_secs(2_u64.pow(ℝ))).await  # 指数退避 2^ℝ
                    continue
                },
                Fail { error: e } => return Err(CrawlerError::NetworkError(e))  # 网络错误 ✗
            }
        }
    }
    
    async micro post(self, url: string, body: &[u8]) -> Result<Response, CrawlerError> {
        let request = Request::builder()
            .method(Method::POST)
            .uri(url)
            .header("Content-Type", "application/json")
            .body(body.to_vec())
            .build()?
            
        self.send_with_retry(request).await
    }
}

enums CrawlerError {
    HttpError(StatusCode),
    NetworkError(HttpError),
    ParseError(string),
    RateLimitExceeded,
    Timeout
}
```

### URL 管理和调度

```valkyrie
use std::collections::{HashSet, VecDeque}
use std::sync::{Arc, Mutex}
use std::hash::{Hash, Hasher}

# URL 优先级队列
class UrlQueue {
    high_priority: VecDeque<CrawlTask>
    normal_priority: VecDeque<CrawlTask>
    low_priority: VecDeque<CrawlTask>
    visited: HashSet<string>
    in_progress: HashSet<string>
}

class CrawlTask {
    url: string
    depth: u32
    priority: Priority
    metadata: TaskMetadata
}

enums Priority {
    High = 3,
    Normal = 2,
    Low = 1
}

class TaskMetadata {
    referrer: string?
    created_at: SystemTime
    retry_count: u32
    custom_headers: HashMap<string, string>
}

imply UrlQueue {
    micro new() -> UrlQueue {
        UrlQueue {
            high_priority: VecDeque::new(),
            normal_priority: VecDeque::new(),
            low_priority: VecDeque::new(),
            visited: HashSet::new(),
            in_progress: HashSet::new()
        }
    }
    
    micro add_url(mut self, url: string, priority: Priority, depth: u32) -> bool {
        if self.visited.contains(&url) || self.in_progress.contains(&url) {
            return false  # URL已存在 ∃
        }
        
        let task = CrawlTask {
            url: url.clone(),
            depth,  # 爬取深度 δ
            priority,  # 优先级 π
            metadata: TaskMetadata {
                referrer: None,
                created_at: SystemTime::now(),  # 创建时间 t₀
                retry_count: 0,  # 重试次数 ℝ
                custom_headers: HashMap::new()
            }
        }
        
        match priority {
            Priority::High => self.high_priority.push_back(task),    # 高优先级队列 Q₃
            Priority::Normal => self.normal_priority.push_back(task), # 普通优先级队列 Q₂
            Priority::Low => self.low_priority.push_back(task)       # 低优先级队列 Q₁
        }
        
        true  # 添加成功 ✓
    }
    
    micro next_task(mut self) -> CrawlTask? {
        if let Some(task) = self.high_priority.pop_front() {
            self.in_progress.insert(task.url.clone())
            return Some(task)
        }
        
        if let Some(task) = self.normal_priority.pop_front() {
            self.in_progress.insert(task.url.clone())
            return Some(task)
        }
        
        if let Some(task) = self.low_priority.pop_front() {
            self.in_progress.insert(task.url.clone())
            return Some(task)
        }
        
        None
    }
    
    micro mark_completed(mut self, url: string) {
        self.in_progress.remove(url)
        self.visited.insert(url.to_string())
    }
    
    micro mark_failed(mut self, url: string) {
        self.in_progress.remove(url)
    }
    
    micro is_empty(self) -> bool {
        self.high_priority.is_empty() && 
        self.normal_priority.is_empty() && 
        self.low_priority.is_empty()
    }
}

# 智能调度器
class CrawlerScheduler {
    queue: Arc<Mutex<UrlQueue>>,
    rate_limiter: RateLimiter,
    domain_delays: HashMap<string, SystemTime>
    
    micro new(requests_per_second: f64) -> CrawlerScheduler {
        CrawlerScheduler {
            queue: Arc::new(Mutex::new(UrlQueue::new())),
            rate_limiter: RateLimiter::new(requests_per_second),
            domain_delays: HashMap::new()
        }
    }
    
    async micro schedule_request(mut self) -> CrawlTask? {
        # 等待速率限制
        self.rate_limiter.wait_if_needed().await
        
        let mut queue = self.queue.lock().unwrap()
        
        # 寻找可以立即处理的任务 ∃
        while let Some(task) = queue.next_task() {
            let 𝒟 = extract_domain(&task.url)?  # 提取域名
            
            # 检查域名延迟 Δt
            if let Some(t_last) = self.domain_delays.get(&𝒟) {
                let Δt_elapsed = SystemTime::now().duration_since(*t_last).unwrap_or_default()
                if Δt_elapsed < Duration::from_secs(1) {
                    # 重新加入队列，降低优先级 π↓
                    queue.add_url(task.url, Priority::Low, task.depth)
                    continue
                }
            }
            
            self.domain_delays.insert(𝒟, SystemTime::now())  # 更新时间戳 t_now
            return Some(task)  # 返回任务 ✓
        }
        
        None
    }
}

# 速率限制器
class RateLimiter {
    interval: Duration
    last_request: SystemTime?
}

imply RateLimiter {
    micro new(requests_per_second: f64) -> RateLimiter {
        RateLimiter {
            interval: Duration::from_secs_f64(1.0 / requests_per_second),
            last_request: None
        }
    }
    
    async micro wait_if_needed(&mut self) {
        if let Some(last) = self.last_request {
            let elapsed = SystemTime::now().duration_since(last).unwrap_or(Duration::ZERO)
            if elapsed < self.interval {
                tokio::time::sleep(self.interval - elapsed).await
            }
        }
        self.last_request = Some(SystemTime::now())
    }
}
```

### HTML 解析和数据提取

```valkyrie
use std::html::{Document, Element, Selector}
use std::regex::Regex

# HTML 解析器
class HtmlParser {
    base_url: string
}

imply HtmlParser {
    micro new(base_url: string) -> HtmlParser {
        HtmlParser { base_url }
    }
    
    micro extract_links(&self, html: string) -> Vec<string> {
        # 使用 CSS 选择器提取链接
        let selector = Selector::parse("a[href]").unwrap()
        let document = Html::parse_document(html)
        
        document.select(&selector)
            .filter_map(|el| el.value().attr("href"))
            .map(|href| self.resolve_url(href).unwrap_or(href.to_string()))
            .collect()
    }
    
    micro extract_text(self, doc: &Document, selector: &str) -> Vec<string> {
        let css_selector = Selector::parse(selector).unwrap()
        let mut texts = Vec::new()
        
        for element in doc.select(&css_selector) {
            let text = element.text().collect::<Vec<_>>().join(" ").trim().to_string()
            if !text.is_empty() {
                texts.push(text)
            }
        }
        
        texts
    }
    
    micro extract_structured_data(self, doc: &Document) -> StructuredData {
        let mut data = StructuredData::new()
        
        # 提取标题
        if let Some(title) = doc.select(&Selector::parse("title").unwrap()).next() {
            data.title = Some(title.text().collect::<String>())
        }
        
        # 提取元数据
        for meta in doc.select(&Selector::parse("meta").unwrap()) {
            if let (Some(name), Some(content)) = (meta.value().attr("name"), meta.value().attr("content")) {
                data.meta.insert(name.to_string(), content.to_string())
            }
        }
        
        # 提取 JSON-LD 结构化数据
        for script in doc.select(&Selector::parse("script[type='application/ld+json']").unwrap()) {
            let json_text = script.text().collect::<String>()
            if let Ok(json_data) = serde_json::from_str::<serde_json::Value>(&json_text) {
                data.json_ld.push(json_data)
            }
        }
        
        data
    }
    
    micro resolve_url(self, href: &str) -> Option<String> {
        if href.starts_with("http://") || href.starts_with("https://") {
            Some(href.to_string())
        } else if href.starts_with("/") {
            Some(format!("{}{}", self.base_url, href))
        } else if href.starts_with("./") {
            Some(format!("{}/{}", self.base_url, &href[2..]))
        } else if !href.starts_with("#") && !href.starts_with("mailto:") && !href.starts_with("javascript:") {
            Some(format!("{}/{}", self.base_url, href))
        } else {
            None
        }
    }
}

class StructuredData {
    title: string?
    meta: HashMap<string, string>
    json_ld: Vec<serde_json::Value>
    custom_fields: HashMap<string, string>
}

imply StructuredData {
    micro new() -> StructuredData {
        StructuredData {
            title: None,
            meta: HashMap::new(),
            json_ld: Vec::new(),
            custom_fields: HashMap::new()
        }
    }
}

enums ParseError {
    HtmlParseError(string),
    SelectorError(string),
    JsonParseError(string)
}
```

### 数据存储和管道

```valkyrie
use std::database::{Database, Connection, Transaction}
use std::fs::{File, OpenOptions}
use std::io::{Write, BufWriter}

# 数据存储接口
trait DataStore {
    async micro save_page(mut self, url: string, data: &CrawledData) -> Result<(), StoreError>
    async micro save_links(mut self, parent_url: string, links: &[string]) -> Result<(), StoreError>
    async micro get_crawled_urls(self) -> Result<HashSet<string>, StoreError>
}

# 抓取结果
class CrawledData {
    url: string
    title: string?
    content: string
    links: Vec<string>
    metadata: StructuredData
    crawled_at: SystemTime
    response_time: Duration
    status_code: u16
}

# 数据库存储实现
class DatabaseStore {
    connection: Connection
}

imply DatabaseStore: DataStore {
    async micro save_page(mut self, url: string, data: &CrawledData) -> Result<(), StoreError> {
        let mut tx = self.connection.begin().await?
        
        # 保存页面数据
        sqlx::query!(
            "INSERT INTO pages (url, title, content, status_code, crawled_at, response_time) 
             VALUES ($1, $2, $3, $4, $5, $6) 
             ON CONFLICT (url) DO UPDATE SET 
             title = EXCLUDED.title, 
             content = EXCLUDED.content, 
             status_code = EXCLUDED.status_code, 
             crawled_at = EXCLUDED.crawled_at, 
             response_time = EXCLUDED.response_time",
            url,
            data.title,
            data.content,
            data.status_code as i32,
            data.crawled_at,
            data.response_time.as_millis() as i64
        ).execute(&mut tx).await?
        
        # 保存元数据
        for (key, value) in &data.metadata.meta {
            sqlx::query!(
                "INSERT INTO page_metadata (url, key, value) VALUES ($1, $2, $3) 
                 ON CONFLICT (url, key) DO UPDATE SET value = EXCLUDED.value",
                data.url, key, value
            ).execute(&mut tx).await?
        }
        
        tx.commit().await?
        Ok(())
    }
    
    async micro save_links(mut self, parent_url: string, links: &[string]) -> Result<(), StoreError> {
        let mut tx = self.connection.begin().await?
        
        for link in links {
            sqlx::query!(
                "INSERT INTO links (parent_url, target_url, discovered_at) 
                 VALUES ($1, $2, $3) 
                 ON CONFLICT (parent_url, target_url) DO NOTHING",
                parent_url, link, SystemTime::now()
            ).execute(&mut tx).await?
        }
        
        tx.commit().await?
        Ok(())
    }
    
    async micro get_crawled_urls(self) -> Result<HashSet<string>, StoreError> {
        let rows = sqlx::query!("SELECT url FROM pages")
            .fetch_all(&self.connection).await?
        
        Ok(rows.into_iter().map(|row| row.url).collect())
    }
}

# JSON 文件存储实现
class JsonFileStore {
    file_path: string
    writer: BufWriter<File>
}

imply JsonFileStore {
    micro new(file_path: string) -> Result<JsonFileStore, StoreError> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?
        
        Ok(JsonFileStore {
            file_path,
            writer: BufWriter::new(file)
        })
    }
}

imply JsonFileStore: DataStore {
    async micro save_page(mut self, url: string, data: &CrawledData) -> Result<(), StoreError> {
        let json_data = serde_json::to_string(data)?
        writeln!(self.writer, "{}", json_data)?
        self.writer.flush()?
        Ok(())
    }
    
    async micro save_links(mut self, parent_url: string, links: &[string]) -> Result<(), StoreError> {
        let link_data = serde_json::json!({
            "parent_url": parent_url,
            "links": links,
            "discovered_at": SystemTime::now()
        })
        
        writeln!(self.writer, "{}", link_data)?
        self.writer.flush()?
        Ok(())
    }
    
    async micro get_crawled_urls(self) -> Result<HashSet<string>, StoreError> {
        # 从文件读取已爬取的 URL
        let content = std::fs::read_to_string(&self.file_path)?
        let mut urls = HashSet::new()
        
        for line in content.lines() {
            if let Ok(data) = serde_json::from_str::<CrawledData>(line) {
                urls.insert(data.url)
            }
        }
        
        Ok(urls)
    }
}

enums StoreError {
    DatabaseError(sqlx::Error),
    IoError(std::io::Error),
    SerializationError(serde_json::Error)
}
```

### 完整的爬虫引擎

```valkyrie
use std::sync::Arc
use tokio::sync::Semaphore

# 主爬虫引擎
class WebCrawler<S: DataStore> {
    client: AsyncHttpClient,
    scheduler: CrawlerScheduler,
    parser: HtmlParser,
    store: S,
    semaphore: Arc<Semaphore>,
    config: CrawlerEngineConfig
}

class CrawlerEngineConfig {
    max_depth: u32
    max_pages: usize?
    allowed_domains: HashSet<string>?
    url_filters: Vec<Regex>
    content_filters: Vec<ContentFilter>
}

class ContentFilter {
    selector: string
    min_length: usize?
    required_keywords: Vec<string>
}

imply<S: DataStore> WebCrawler<S> {
    micro new(config: CrawlerConfig, store: S) -> WebCrawler<S> {
        let client = AsyncHttpClient::new(config.clone()).await
        let scheduler = CrawlerScheduler::new(config.requests_per_second)
        
        WebCrawler {
            client,
            scheduler,
            parser: HtmlParser::new(String::new()),
            store,
            semaphore: Arc::new(Semaphore::new(config.max_concurrent)),
            config: CrawlerEngineConfig::default()
        }
    }
    
    async micro crawl(mut self, seed_urls: Vec<string>) -> Result<CrawlStats, CrawlerError> {
        let mut stats = CrawlStats::new()
        
        # 添加种子 URL
        for url in seed_urls {
            self.scheduler.queue.lock().unwrap().add_url(url, Priority::High, 0)
        }
        
        # 获取已爬取的 URL
        let crawled_urls = self.store.get_crawled_urls().await?
        
        # 主爬取循环
        while let Some(task) = self.scheduler.schedule_request().await {
            if crawled_urls.contains(&task.url) {
                continue
            }
            
            if let Some(max_pages) = self.config.max_pages {
                if stats.pages_crawled >= max_pages {
                    break
                }
            }
            
            # 获取并发许可
            let permit = self.semaphore.clone().acquire_owned().await.unwrap()
            
            let client = self.client.clone()
            let store = self.store.clone()
            let parser = self.parser.clone()
            let scheduler = self.scheduler.clone()
            let config = self.config.clone()
            
            # 异步处理单个页面
            tokio::spawn(async move {
                let _permit = permit  # 确保许可在任务结束时释放
                
                match Self::crawl_single_page(client, store, parser, task, config).await {
                    Fine { value: page_data } => {
                        # 添加发现的链接到队列
                        for link in &page_data.links {
                            if page_data.depth < config.max_depth {
                                scheduler.queue.lock().unwrap().add_url(
                                    link.clone(), 
                                    Priority::Normal, 
                                    page_data.depth + 1
                                )
                            }
                        }
                        
                        scheduler.queue.lock().unwrap().mark_completed(&page_data.url)
                    },
                    Fail { error: e } => {
                        print("Failed to crawl {}: {:?}", task.url, e)
                        scheduler.queue.lock().unwrap().mark_failed(&task.url)
                    }
                }
            })
            
            stats.pages_crawled += 1  # 页面计数 Σ++
        }
        
        Ok(stats)
    }
    
    async micro crawl_single_page(
        client: AsyncHttpClient,
        mut store: S,
        parser: HtmlParser,
        task: CrawlTask,
        config: CrawlerEngineConfig
    ) -> Result<CrawledPageData, CrawlerError> {
        let start_time = SystemTime::now()
        
        # 发送 HTTP 请求
        let response = client.get(&task.url).await?
        let response_time = SystemTime::now().duration_since(start_time).unwrap_or_default()
        
        # 检查内容类型
        let content_type = response.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
        
        if !content_type.contains("text/html") {
            return Err(CrawlerError::UnsupportedContentType(content_type.to_string()))
        }
        
        # 解析 HTML
        let html = response.text().await?
        let doc = parser.parse(&html)?
        
        # 提取数据
        let links = parser.extract_links(&doc)
        let structured_data = parser.extract_structured_data(&doc)
        let content = parser.extract_text(&doc, "body").join("\n")
        
        # 应用内容过滤器
        if !Self::passes_content_filters(&content, &config.content_filters) {
            return Err(CrawlerError::ContentFiltered)
        }
        
        # 过滤链接
        let filtered_links = Self::filter_links(&links, &config)
        
        let crawled_data = CrawledData {
            url: task.url.clone(),
            title: structured_data.title.clone(),
            content,
            links: filtered_links.clone(),
            metadata: structured_data,
            crawled_at: SystemTime::now(),
            response_time,
            status_code: response.status().as_u16()
        }
        
        # 保存数据
        store.save_page(&task.url, &crawled_data).await?
        store.save_links(&task.url, &filtered_links).await?
        
        Ok(CrawledPageData {
            url: task.url,
            links: filtered_links,
            depth: task.depth
        })
    }
    
    micro filter_links(links: &[string], config: &CrawlerEngineConfig) -> Vec<string> {
        links.iter()
            .filter(|link| {
                # 域名过滤
                if let Some(allowed_domains) = &config.allowed_domains {
                    if let Some(domain) = extract_domain(link) {
                        if !allowed_domains.contains(&domain) {
                            return false
                        }
                    }
                }
                
                # URL 模式过滤
                for filter in &config.url_filters {
                    if filter.is_match(link) {
                        return false
                    }
                }
                
                true
            })
            .cloned()
            .collect()
    }
    
    micro passes_content_filters(content: &str, filters: &[ContentFilter]) -> bool {
        for filter in filters {
            if let Some(min_length) = filter.min_length {
                if content.len() < min_length {
                    return false
                }
            }
            
            for keyword in &filter.required_keywords {
                if !content.to_lowercase().contains(&keyword.to_lowercase()) {
                    return false
                }
            }
        }
        
        true
    }
}

class CrawledPageData {
    url: string
    links: Vec<string>
    depth: u32
}

class CrawlStats {
    pages_crawled: usize
    links_discovered: usize
    errors: usize
    start_time: SystemTime
}

imply CrawlStats {
    micro new() -> CrawlStats {
        CrawlStats {
            pages_crawled: 0,
            links_discovered: 0,
            errors: 0,
            start_time: SystemTime::now()
        }
    }
}
```

## 使用示例

### 基础爬虫

```valkyrie
async micro basic_crawler_example() -> Result<(), Box<dyn std::error::Error>> {
    # 配置爬虫
    let config = CrawlerConfig {
        max_concurrent: 10,
        request_delay: Duration::from_millis(100),
        timeout: Duration::from_secs(30),
        user_agent: "Valkyrie-Crawler/1.0".to_string(),
        max_retries: 3
    }
    
    # 创建数据存储
    let store = JsonFileStore::new("crawled_data.jsonl".to_string())?
    
    # 创建爬虫实例
    let mut crawler = WebCrawler::new(config, store)
    
    # 设置爬虫引擎配置
    crawler.config = CrawlerEngineConfig {
        max_depth: 3,
        max_pages: Some(1000),
        allowed_domains: Some(hashset!["example.com".to_string()]),
        url_filters: vec![
            Regex::new(r"\.(jpg|jpeg|png|gif|pdf)$")?,
            Regex::new(r"/admin/")?
        ],
        content_filters: vec![
            ContentFilter {
                selector: "body".to_string(),
                min_length: Some(100),
                required_keywords: vec![]
            }
        ]
    }
    
    # 开始爬取
    let seed_urls = vec![
        "https://example.com".to_string(),
        "https://example.com/blog".to_string()
    ]
    
    let stats = crawler.crawl(seed_urls).await?
    
    print("爬取完成:")
    print("  页面数: {}", stats.pages_crawled)
    print("  链接数: {}", stats.links_discovered)
    print("  错误数: {}", stats.errors)
    
    Ok(())
}
```

### 电商数据爬取

```valkyrie
class ProductData {
    name: string
    price: f64?
    description: string
    images: Vec<string>
    specifications: HashMap<string, string>
    reviews_count: u32?
    rating: f32?
}

class EcommerceCrawler {
    base_crawler: WebCrawler<DatabaseStore>
    product_selectors: ProductSelectors
}

class ProductSelectors {
    name: string
    price: string
    description: string
    images: string
    specifications: string
    reviews: string
}

imply EcommerceCrawler {
    async micro crawl_products(mut self, category_urls: Vec<string>) -> Result<Vec<ProductData>, CrawlerError> {
        let mut products = Vec::new()
        
        for category_url in category_urls {
            let response = self.base_crawler.client.get(&category_url).await?
            let html = response.text().await?
            let doc = self.base_crawler.parser.parse(&html)?
            
            # 提取产品链接
            let product_links = self.extract_product_links(&doc)
            
            # 爬取每个产品页面
            for product_url in product_links {
                if let Ok(product) = self.crawl_single_product(&product_url).await {
                    products.push(product)
                }
                
                # 添加延迟避免被封
                tokio::time::sleep(Duration::from_millis(500)).await
            }
        }
        
        Ok(products)
    }
    
    async micro crawl_single_product(self, url: &str) -> Result<ProductData, CrawlerError> {
        let response = self.base_crawler.client.get(url).await?
        let html = response.text().await?
        let doc = self.base_crawler.parser.parse(&html)?
        
        let name = self.extract_product_name(&doc)
        let price = self.extract_product_price(&doc)
        let description = self.extract_product_description(&doc)
        let images = self.extract_product_images(&doc)
        let specifications = self.extract_specifications(&doc)
        let (reviews_count, rating) = self.extract_reviews_info(&doc)
        
        Ok(ProductData {
            name,
            price,
            description,
            images,
            specifications,
            reviews_count,
            rating
        })
    }
}
```

### 新闻聚合爬虫

```valkyrie
class NewsArticle {
    title: string
    content: string
    author: string?
    published_at: SystemTime?
    source: string
    category: string?
    tags: Vec<string>
    url: string
}

class NewsCrawler {
    base_crawler: WebCrawler<DatabaseStore>
    news_sources: Vec<NewsSource>
}

class NewsSource {
    name: string
    base_url: string
    article_selector: string
    title_selector: string
    content_selector: string
    author_selector: string
    date_selector: string
    date_format: string
}

imply NewsCrawler {
    async micro crawl_latest_news(mut self) -> Result<Vec<NewsArticle>, CrawlerError> {
        let mut articles = Vec::new()
        
        for source in &self.news_sources {
            match self.crawl_news_source(source).await {
                Ok(mut source_articles) => articles.append(&mut source_articles),
                Fail { error: e } => print("Failed to crawl {}: {:?}", source.name, e)
            }
        }
        
        # 去重和排序
        articles.sort_by(|a, b| b.published_at.cmp(&a.published_at))
        articles.dedup_by(|a, b| a.title == b.title)
        
        Ok(articles)
    }
    
    async micro crawl_news_source(self, source: &NewsSource) -> Result<Vec<NewsArticle>, CrawlerError> {
        let response = self.base_crawler.client.get(&source.base_url).await?
        let html = response.text().await?
        let doc = self.base_crawler.parser.parse(&html)?
        
        let article_links = self.extract_article_links(&doc, &source.article_selector)
        let mut articles = Vec::new()
        
        for link in article_links {
            if let Ok(article) = self.crawl_single_article(&link, source).await {
                articles.push(article)
            }
        }
        
        Ok(articles)
    }
}
```

## 高级特性

### 分布式爬取

```valkyrie
use std::redis::{RedisClient, RedisConnection}

# 分布式任务队列
class DistributedCrawler {
    redis: RedisConnection,
    worker_id: string,
    local_crawler: WebCrawler<DatabaseStore>
}

imply DistributedCrawler {
    async micro run_worker(mut self) -> Result<(), CrawlerError> {
        loop {
            # 从 Redis 获取任务
            if let Some(task_json) = self.redis.blpop("crawl_queue", 10).await? {
                let task: CrawlTask = serde_json::from_str(&task_json)?
                
                # 处理任务
                match self.process_task(task).await {
                    Fine { value: result } => {
                        # 将结果推送到结果队列
                        let result_json = serde_json::to_string(&result)?
                        self.redis.lpush("crawl_results", result_json).await?
                    },
                    Fail { error: e } => {
                        # 错误处理和重试逻辑
                        print("Task failed: {:?}", e)
                    }
                }
            }
        }
    }
    
    async micro add_urls_to_queue(mut self, urls: Vec<String>) -> Result<(), CrawlerError> {
        for url in urls {
            let task = CrawlTask {
                url,
                depth: 0,
                priority: Priority::Normal,
                metadata: TaskMetadata::default()
            }
            
            let task_json = serde_json::to_string(&task)?
            self.redis.lpush("crawl_queue", task_json).await?
        }
        
        Ok(())
    }
}
```

### 智能反爬虫

```valkyrie
# 反爬虫检测和规避
class AntiDetectionCrawler {
    user_agents: Vec<string>,
    proxy_pool: Vec<string>,
    session_manager: SessionManager
}

class SessionManager {
    sessions: HashMap<string, CrawlSession>
    max_requests_per_session: u32
}

class CrawlSession {
    cookies: HashMap<string, string>
    headers: HashMap<string, string>
    request_count: u32
    created_at: SystemTime
}

imply AntiDetectionCrawler {
    async micro crawl_with_evasion(mut self, url: &str) -> Result<Response, CrawlerError> {
        # 随机选择 User-Agent
        let user_agent = self.user_agents.choose(&mut rand::thread_rng()).unwrap()
        
        # 获取或创建会话
        let session = self.session_manager.get_or_create_session(url)
        
        # 构建请求
        let mut request = Request::builder()
            .uri(url)
            .header("User-Agent", user_agent)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header("Accept-Language", "en-US,en;q=0.5")
            .header("Accept-Encoding", "gzip, deflate")
            .header("Connection", "keep-alive")
            .header("Upgrade-Insecure-Requests", "1")
        
        # 添加会话 cookies
        if !session.cookies.is_empty() {
            let cookie_header = session.cookies.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("; ")
            request = request.header("Cookie", cookie_header)
        }
        
        # 添加随机延迟
        let delay = rand::thread_rng().gen_range(1000..5000)
        tokio::time::sleep(Duration::from_millis(delay)).await
        
        # 发送请求
        let response = self.send_request(request.build()?).await?
        
        # 更新会话
        self.session_manager.update_session(url, &response)
        
        Ok(response)
    }
}
```

Valkyrie 的网络爬虫框架提供了从简单数据采集到大规模分布式爬取的完整解决方案，具有高性能、可扩展、易维护的特点，适用于各种数据采集场景。