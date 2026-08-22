CREATE TABLE singleton (
    _singleton_guard INTEGER
        NOT NULL
        DEFAULT 1
        CHECK (_singleton_guard = 1)
        UNIQUE
);
INSERT INTO singleton DEFAULT VALUES;

CREATE TABLE page_crawl (
    id INTEGER PRIMARY KEY,
    -- The URL of the webpage this crawl was on
    page_url TEXT,
    -- When this crawl was performed, in seconds since the Unix epoch
    timestamp_unix_secs INTEGER
);

CREATE TABLE pending_page_crawl (
    id INTEGER PRIMARY KEY,
    -- The URL of the webpage this pending crawl should be attempted on
    page_url TEXT,
    -- The earliest point in time that this pending crawl should be attempted, in seconds since the Unix epoch, or NULL if unrestricted
    effective_start_timestamp_unix_secs INTEGER NULL
);

CREATE TABLE crawled_content (
    id INTEGER PRIMARY KEY,
    page_crawl_id INTEGER,
    -- The name of the subrule that crawled this content
    crawler_subrule_name TEXT,
    -- The path this content was crawled to, relative to the crawl output directory
    output_path TEXT UNIQUE,
    -- The size of the saved content's data, in bytes
    output_size_bytes INTEGER,
    -- The 256-bit BLAKE3 checksum value of the saved content's data
    output_checksum_blake3 BLOB,
    FOREIGN KEY(page_crawl_id) REFERENCES page_crawl(id)
);
