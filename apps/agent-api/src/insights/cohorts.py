import pandas as pd
import numpy as np
import json
from sklearn.cluster import DBSCAN
from sklearn.mixture import GaussianMixture
from sklearn.ensemble import RandomForestClassifier
from xgboost import XGBClassifier
from utils.store import pool  # PG ConnectionPool

def fetch_data():
    """Fetch visitor events and lead statuses from the database."""
    query = """
    SELECT v.id AS visitor_id, v.lead_status, v.country, e.data::text AS event_data, e.created_at
    FROM visitors v
    LEFT JOIN events e ON v.id = e.visitor_id
    ORDER BY v.id, e.created_at
    """
    with pool.getconn() as conn:
        df = pd.read_sql(query, conn)
        pool.putconn(conn)
    return df

def extract_event_features(df):
    """Parse JSON event data, detect sessions (ensuring first event is a page visit), and generate numerical features."""
    def parse_json(event_str):
        try:
            event = json.loads(event_str)
            return {
                "event_type": event.get("event", "unknown"),
                "device_type": event.get("deviceType", "unknown"),
                "utm_campaign": 1 if event.get("utmParams", {}).get("campaign") else 0,
                "utm_source": 1 if event.get("utmParams", {}).get("source") else 0,
                "utm_medium": 1 if event.get("utmParams", {}).get("medium") else 0,
                "ad_click": 1 if event.get("adClickIds") else 0,
                "product_price": float(event.get("productData", {}).get("price", 0)),
                "new_lead": 1 if event.get("newLead") else 0,
                "path": event.get("path", "unknown"),
            }
        except json.JSONDecodeError:
            return None

    # Parse JSON fields
    df["parsed_data"] = df["event_data"].apply(parse_json)
    df = df.dropna(subset=["parsed_data"])
    feature_df = pd.json_normalize(df["parsed_data"])
    
    df = df.join(feature_df)
    df.drop(columns=["event_data", "parsed_data"], inplace=True)

    # Convert timestamp to datetime
    df["created_at"] = pd.to_datetime(df["created_at"])
    
    # **Sort by visitor and timestamp**
    df = df.sort_values(by=["visitor_id", "created_at"])

    # **Session Identification: Only Start a New Session if the Next Event is a Page Visit**
    session_gap = pd.Timedelta(minutes=30)
    df["time_diff"] = df.groupby("visitor_id")["created_at"].diff()
    df["new_session"] = (df["time_diff"] > session_gap) & (df["event_type"] == "page_visit")  # ✅ Ensures only `page_visit` starts a new session
    df["session_id"] = df.groupby("visitor_id")["new_session"].cumsum().astype(int)
    df["session_id"] = df["visitor_id"].astype(str) + "_" + df["session_id"].astype(str)  # Unique session ID

    # **Aggregate Features Per Session**
    agg_df = df.groupby("session_id").agg({
        "visitor_id": "first",  # Keep visitor ID reference
        "product_price": ["sum", "mean"],
        "new_lead": "sum",
        "utm_campaign": "sum",
        "utm_source": "sum",
        "utm_medium": "sum",
        "ad_click": "sum",
        "created_at": ["min", "max", "count"],  # first event, last event, event count
        "path": lambda x: " → ".join(x.dropna())  # Reconstruct session journey
    })
    
    # Rename columns for clarity
    agg_df.columns = ["_".join(col) for col in agg_df.columns]

    # **Compute session duration**
    agg_df["session_duration"] = (agg_df["created_at_max"] - agg_df["created_at_min"]).dt.total_seconds()

    # Drop unnecessary timestamp columns
    agg_df.drop(columns=["created_at_min", "created_at_max"], inplace=True)

    # ✅ Reset index to avoid aggregation issues
    return agg_df.reset_index()


def cluster_visitors(features):
    """Apply DBSCAN and GMM clustering to identify behavioral cohorts."""
     # Ensure only numeric data is passed to DBSCAN/GMM
    non_numeric_columns = ["visitor_id", "path"]

    # Drop only existing columns to avoid KeyErrors
    existing_cols = [col for col in non_numeric_columns if col in features.columns]
    X = features.drop(columns=existing_cols, errors="ignore")

    # Ensure all remaining columns are numeric
    X = X.select_dtypes(include=[np.number])

    # DBSCAN clustering
    dbscan = DBSCAN(eps=10, min_samples=5).fit(X)
    features["dbscan_label"] = dbscan.labels_

    best_gmm = None
    lowest_bic = np.inf
    best_n_components = 1

    for n in range(2, min(10, len(X))):  # Try cluster sizes from 2 to max 10 (or data size)
        gmm = GaussianMixture(n_components=n, random_state=42)
        gmm.fit(X)

        bic = gmm.bic(X)
        if bic < lowest_bic:  # Keep track of the best cluster count
            lowest_bic = bic
            best_gmm = gmm
            best_n_components = n

    # **Fit GMM with the best number of clusters**
    features["gmm_label"] = best_gmm.fit_predict(X)

    # GMM clustering
    # gmm = GaussianMixture(n_components=3, random_state=42)
    # features["gmm_label"] = gmm.fit_predict(X)

    return features

def classify_cohorts(features):
    """Train an XGBoost model to classify visitors into behavioral cohorts."""
    # Ensure 'path' exists before processing
    if "path" in features.columns:
        # Convert NaNs to empty strings to avoid errors
        features["path"] = features["path"].fillna("")

        # **TF-IDF Vectorization of Path Journeys**
        vectorizer = TfidfVectorizer(ngram_range=(1, 3), max_features=500)  # Use unigrams, bigrams, trigrams
        path_features = vectorizer.fit_transform(features["path"]).toarray()

        # Convert TF-IDF output to DataFrame
        path_feature_names = [f"path_tfidf_{i}" for i in range(path_features.shape[1])]
        path_df = pd.DataFrame(path_features, columns=path_feature_names, index=features.index)

        # Merge with main features
        features = pd.concat([features, path_df], axis=1)

    # Drop non-numeric columns (visitor_id, path as raw text)
    non_numeric_columns = ["visitor_id", "path"]
    existing_cols = [col for col in non_numeric_columns if col in features.columns]
    
    X = features.drop(columns=existing_cols, errors="ignore")  # ✅ Drops `path` (raw text) but keeps TF-IDF features
    
    # Ensure X only contains numeric data
    X = X.select_dtypes(include=[np.number])  # ✅ Select only numerical types

    y = features["gmm_label"]  # Target: GMM cluster label

    # Train classifier (XGBoost)
    model = XGBClassifier()
    model.fit(X, y)

    return model, features

def assign_cohorts_to_sessions(classified_data):
    """Stores the mapping of session → visitor → cohort for future reference."""

    # Ensure the required columns exist
    required_cols = ["session_id", "visitor_id", "gmm_label"]
    missing_cols = [col for col in required_cols if col not in classified_data.columns]
    if missing_cols:
        raise ValueError(f"Missing columns: {missing_cols}")

    # Store session-to-cohort mapping
    session_to_cohort = classified_data[["session_id", "visitor_id", "gmm_label"]]

    return session_to_cohort

def get_latest_visitor_cohort(visitor_id, session_to_cohort):
    """Retrieve the latest assigned cohort for a given visitor."""
    
    visitor_sessions = session_to_cohort[session_to_cohort["visitor_id"] == visitor_id]
    if visitor_sessions.empty:
        return None  # No session data available for this visitor
    
    latest_session = visitor_sessions.sort_values(by="session_id").iloc[-1]
    return latest_session["gmm_label"]