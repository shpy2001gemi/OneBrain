"""
Download Wikidata dump with robust resume support.

Uses curl with -C - (auto-resume) in a retry loop.
Much more reliable than Python requests for 100GB+ files.
"""
import subprocess
import sys
import time
import os
from pathlib import Path

DUMP_URL = "https://dumps.wikimedia.org/wikidatawiki/entities/20260706/wikidata-20260706-all.json.gz"
DEST = Path(__file__).parent / "checkpoints" / "wikidata-20260706-all.json.gz"
MAX_RETRIES = 100
RETRY_WAIT = 30  # seconds

def main():
    DEST.parent.mkdir(parents=True, exist_ok=True)
    
    for attempt in range(1, MAX_RETRIES + 1):
        current_size = DEST.stat().st_size if DEST.exists() else 0
        print(f"\n{'='*60}")
        print(f"Attempt {attempt}/{MAX_RETRIES}")
        print(f"Current size: {current_size / 1e9:.2f} GB")
        print(f"{'='*60}")
        
        # curl with resume, progress bar, retry, and timeout
        cmd = [
            "curl",
            "-C", "-",           # Auto-resume from where left off
            "-L",                # Follow redirects
            "-o", str(DEST),     # Output file
            "--retry", "5",      # curl internal retry
            "--retry-delay", "10",
            "--connect-timeout", "30",
            "--speed-limit", "10000",   # Abort if < 10KB/s
            "--speed-time", "60",       # for 60 seconds
            "-H", "User-Agent: OneBrain/1.0 (concept-registry)",
            DUMP_URL,
        ]
        
        result = subprocess.run(cmd)
        
        if result.returncode == 0:
            final_size = DEST.stat().st_size
            print(f"\nDownload complete! Size: {final_size / 1e9:.1f} GB")
            print(f"Path: {DEST}")
            return
        
        # Check if we made progress
        new_size = DEST.stat().st_size if DEST.exists() else 0
        progress = new_size - current_size
        print(f"\ncurl exited with code {result.returncode}")
        print(f"Progress this attempt: +{progress / 1e6:.1f} MB")
        print(f"Total downloaded: {new_size / 1e9:.2f} GB")
        print(f"Retrying in {RETRY_WAIT}s...")
        time.sleep(RETRY_WAIT)
    
    print(f"\nMax retries ({MAX_RETRIES}) exceeded!")
    sys.exit(1)

if __name__ == "__main__":
    main()
