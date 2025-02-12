import pandas as pd
from utils.sql import engine
from sqlalchemy import text


def get_journeys():
    query = """
    WITH ranked_events AS (
        SELECT 
            e.visitor_id,
            e.created_at,
            e.data->>'event' AS data_event,
            e.data->>'path' AS data_path,
            COALESCE(e.data->'formData' ? 'email', FALSE) AS has_email,
            e.data->'formData'->>'formName' AS data_form_name,
            ROW_NUMBER() OVER (
                PARTITION BY e.visitor_id, e.data->>'event', e.data->>'path', DATE_TRUNC('second', e.created_at)
                ORDER BY e.created_at
            ) AS rn
        FROM events e
        WHERE e.data->>'event' IN ('page_visit', 'form_submission')
    ),
    session_markers AS (
        SELECT 
            re.*,
            LAG(re.created_at) OVER (
                PARTITION BY re.visitor_id ORDER BY re.created_at
            ) AS prev_event_time,
            LAG(re.data_event) OVER (
                PARTITION BY re.visitor_id ORDER BY re.created_at
            ) AS prev_event
        FROM ranked_events re
        WHERE rn = 1
    ),
    sessions AS (
        SELECT 
            *,
            SUM(
                CASE 
                    WHEN prev_event_time IS NULL THEN 1  -- First event starts a new session
                    WHEN data_event = 'page_visit' AND created_at - prev_event_time > INTERVAL '30 minutes' THEN 1
                    ELSE 0 
                END
            ) OVER (PARTITION BY visitor_id ORDER BY created_at) AS session_id
        FROM session_markers
    ),
    session_journeys AS (
        SELECT 
            visitor_id,
            session_id,
            ARRAY_AGG(
                CASE 
                    WHEN data_event = 'form_submission' THEN 
                        'form_submission:' || COALESCE(data_form_name, '[unknown_form]')
                    ELSE 
                        data_event || ':' || data_path
                END
                ORDER BY created_at
            ) AS journey
        FROM sessions
        GROUP BY visitor_id, session_id
    ),
    journey_counts AS (
        SELECT 
            journey,
            COUNT(*) AS journey_count
        FROM session_journeys
        GROUP BY journey
        ORDER BY journey_count DESC
    )
    SELECT journey, journey_count
    FROM journey_counts
    WHERE ARRAY_TO_STRING(journey, ' -> ') LIKE '%%form_submission:%%';
    """
    df = pd.read_sql(query, engine)
    return df


def compute_dropoffs(data: dict):
    for funnel in data.get("funnels", []):
        critical_path = funnel.get("critical_path", [])
        if len(critical_path) < 2:
            continue

        funnel_steps_sql = (
            "ARRAY["
            + ", ".join(f"'{step.replace('\'', '\'\'')}'" for step in critical_path)
            + "]"
        )

        query = f"""
        WITH ranked_events AS (
            SELECT 
                e.visitor_id,
                e.created_at,
                e.data->>'event' AS event,
                e.data->>'path' AS path,
                e.data->'formData' AS formData,
                ROW_NUMBER() OVER (
                    PARTITION BY e.visitor_id, e.data->>'event', e.data->>'path', DATE_TRUNC('second', e.created_at)
                    ORDER BY e.created_at
                ) AS rn
            FROM events e
            WHERE e.data->>'event' IN ('page_visit', 'form_submission')
        ),
        session_markers AS (
            SELECT 
                re.visitor_id,
                re.created_at,
                re.event,
                re.path,
                re.formData,
                LAG(re.created_at) OVER (PARTITION BY re.visitor_id ORDER BY re.created_at) AS prev_created_at
            FROM ranked_events re
            WHERE rn = 1
        ),
        sessions AS (
            SELECT 
                visitor_id,
                created_at,
                event,
                path,
                formData,
                CASE 
                    WHEN prev_created_at IS NULL OR created_at - prev_created_at > INTERVAL '30 minutes'
                    THEN 1 ELSE 0
                END AS new_session
            FROM session_markers
        ),
        session_ids AS (
            SELECT 
                visitor_id,
                created_at,
                event,
                path,
                formData,
                SUM(new_session) OVER (PARTITION BY visitor_id ORDER BY created_at) AS session_id
            FROM sessions
        ),
        session_steps AS (
            SELECT 
                session_id,
                visitor_id,
                ARRAY_AGG(
                    CASE 
                        WHEN event = 'form_submission'
                            THEN 'form_submission:' || COALESCE(formData->>'formName', '[unknown_form]')
                        ELSE event || ':' || path
                    END
                    ORDER BY created_at
                ) AS journey
            FROM session_ids
            GROUP BY visitor_id, session_id
        ),
        funnel_steps AS (
            SELECT t.step, t.step_idx,
                CASE WHEN t.step LIKE '%%*%%' THEN TRUE ELSE FALSE END AS is_wildcard,
                CASE WHEN t.step LIKE '%%*%%' THEN REPLACE(t.step, '*', '') ELSE t.step END AS match_value
            FROM UNNEST({funnel_steps_sql}) WITH ORDINALITY AS t(step, step_idx)
        ),
        aggregated AS (
            SELECT 
                fs.step_idx,
                fs.step AS from_step,
                COUNT(DISTINCT s.session_id) AS reached_from_step
            FROM funnel_steps fs
            LEFT JOIN session_steps s ON (
                (fs.is_wildcard AND s.journey::text ILIKE '%%' || fs.match_value || '%%')
                OR (NOT fs.is_wildcard AND s.journey @> ARRAY[fs.step])
            )
            GROUP BY fs.step_idx, fs.step
        )
        SELECT from_step, to_step, reached_from_step, reached_to_step, dropoff_rate
        FROM (
            SELECT 
                a.from_step,
                LEAD(a.from_step) OVER (ORDER BY a.step_idx) AS to_step,
                a.reached_from_step,
                COALESCE(LEAD(a.reached_from_step) OVER (ORDER BY a.step_idx), 0) AS reached_to_step,
                ROUND(
                    (a.reached_from_step - COALESCE(LEAD(a.reached_from_step) OVER (ORDER BY a.step_idx), 0))
                    * 100.0 / a.reached_from_step, 2
                ) AS dropoff_rate,
                a.step_idx
            FROM aggregated a
        ) final
        WHERE to_step IS NOT NULL
        ORDER BY step_idx;
        """

        df = pd.read_sql(query, engine)
        funnel["dropoff_analysis"] = df.to_dict(orient="records")
    return data
