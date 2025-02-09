import pandas as pd
from typing import List

def get_top_paths(session_paths: List[dict]):
    session_paths = pd.DataFrame(session_paths)  # Convert back to DataFrame

    # Aggregate visitor paths
    path_counts = session_paths["page_journey"].value_counts().reset_index()
    path_counts.columns = ["visitor_path", "count"]

    # Keep top 10 most frequent visitor paths
    return path_counts.head(10)

def compute_dropoffs(funnels, session_paths: List[dict]):
    """Calculate drop-off rates for each funnel step."""
    session_df = pd.DataFrame(session_paths)
    dropoff_analysis = []

    for funnel in funnels:
        path = funnel["visitor_path"]
        if isinstance(path, str):  
            path = [p.strip() for p in path.split("→")]
        total_visitors = funnel["count"]

        if len(path) < 2:
            continue

        step_dropoffs = []
        for i in range(len(path) - 1):
            from_page = path[i]
            to_page = path[i + 1]

            # Count sessions that reached 'from_page' but not 'to_page'
            step_visitors = session_df[session_df["page_journey"].str.contains(from_page, na=False)]
            next_step_visitors = session_df[session_df["page_journey"].str.contains(to_page, na=False)]

            visitors_lost = len(step_visitors) - len(next_step_visitors)
            dropoff_rate = visitors_lost / max(len(step_visitors), 1)  # Avoid division by zero

            step_dropoffs.append({
                "from_page": from_page,
                "to_page": to_page,
                "dropoff_rate": round(dropoff_rate * 100, 2)  # Convert to percentage
            })

        dropoff_analysis.append({
            "funnel": path,
            "total_visitors": total_visitors,
            "step_dropoffs": step_dropoffs
        })

    return dropoff_analysis