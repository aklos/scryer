import httpx
import json
# from urllib.robotparser import RobotFileParser
from parsel import Selector

headers = {
    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36"
}

def find_site_maps(site_url: str):
    # rp = RobotFileParser(site_url + '/robots.txt')
    # rp.read()
    # print(rp.sitemaps)
    # print(site_url + '/robots.txt', rp.site_maps())
    # return rp.site_maps()
    response = httpx.get(site_url + '/robots.txt', headers=headers, timeout=10, follow_redirects=True)
    site_maps = []
    for line in response.text.splitlines():
        if line.strip().startswith('Sitemap:'):
            site_maps.append(line.split('Sitemap:')[1].strip())
    return site_maps

def fetch_site_map(url: str):
    response = httpx.get(url, headers=headers, timeout=10, follow_redirects=True)
    selector = Selector(response.text)
    data = []
    for url in selector.xpath('//url'):
        location = url.xpath('loc/text()').get()
        modified = url.xpath('lastmod/text()').get()
        data.append({ "location": location, "modified": modified })
    return data

def fetch_site_landing(url: str):
    response = httpx.get(url, headers=headers, timeout=10, follow_redirects=True)
    return response.text

def extract_site_info(site_url: str):
    site_maps = find_site_maps(site_url)
    site_map_locations = []
    for site_map in site_maps:
        site_map_locations = site_map_locations + fetch_site_map(site_map)
    
    landing_page = fetch_site_landing(site_url)

    return [site_map_locations, landing_page]