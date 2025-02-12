import pandas as pd
from typing import List
from utils.sql import engine


def get_engagement(site_map: List[dict]):
    query = """
    SELECT
        COUNT(*),
        e.data->>'path' AS page
    FROM events e
    GROUP BY e.data->>'path'
    """
    df = pd.read_sql(query, engine)
    hits = df.to_dict(orient="records")

    for entry in site_map:
        hit = [x for x in hits if entry["location"].endswith(x["page"])]
        if hit:
            hit = hit[0]
            entry["hits"] = hit["count"]
        else:
            entry["hits"] = 0
    return site_map
