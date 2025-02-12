import pandas as pd
from utils.sql import engine


def get_attributions():
    query = """
    WITH deduped_events AS (
        SELECT 
            e.visitor_id,
            e.created_at,
            e.data,
            ROW_NUMBER() OVER (
            PARTITION BY e.visitor_id, e.data->>'event', DATE_TRUNC('second', e.created_at)
            ORDER BY e.created_at
            ) AS rn
        FROM events e
        WHERE e.data->>'event' IN ('page_visit', 'form_submission', 'conversion')
    ),
    lagged AS (
        SELECT
            visitor_id,
            created_at,
            data,
            LAG(created_at) OVER (PARTITION BY visitor_id ORDER BY created_at) AS prev_created_at
        FROM deduped_events
        WHERE rn = 1
    ),
    session_flags AS (
        SELECT
            visitor_id,
            created_at,
            data,
            CASE 
            WHEN prev_created_at IS NULL OR created_at - prev_created_at > INTERVAL '30 minutes'
            THEN 1 ELSE 0
            END AS new_session
        FROM lagged
    ),
    session_data AS (
        SELECT
            visitor_id,
            created_at,
            data,
            SUM(new_session) OVER (PARTITION BY visitor_id ORDER BY created_at) AS session_id
        FROM session_flags
    ),
    attributed_visits AS (
        SELECT
            s.visitor_id,
            s.created_at AS attributed_at,
            s.session_id,
            s.data->'utmParams'->>'campaign' AS utm_campaign,
            ad.ad_source,
            ad.ad_click_id
        FROM session_data s
        LEFT JOIN LATERAL (
            SELECT
            t.t_key AS ad_source,
            t.t_value AS ad_click_id
            FROM jsonb_each_text(s.data->'adClickIds') AS t(t_key, t_value)
            LIMIT 1
        ) ad ON s.data ? 'adClickIds'
        WHERE s.data ? 'utmParams' OR s.data ? 'adClickIds'
    ),
    conversions AS (
        SELECT 
            sd.visitor_id,
            sd.created_at AS conversion_at,
            sd.session_id,
            CASE WHEN sd.data->>'event' = 'conversion' THEN 'conversion' ELSE 'lead' END AS conversion_type
        FROM session_data sd
        WHERE sd.data->>'newLead' = 'true' OR sd.data->>'event' = 'conversion'
    )
    SELECT
        a.visitor_id,
        a.attributed_at,
        a.session_id,
        a.utm_campaign,
        a.ad_source,
        a.ad_click_id,
        c.conversion_at,
        c.conversion_type
    FROM attributed_visits a
    LEFT JOIN conversions c 
    ON a.visitor_id = c.visitor_id 
    AND a.session_id = c.session_id 
    AND c.conversion_at > a.attributed_at
    ORDER BY a.visitor_id, a.attributed_at;
    """
    df = pd.read_sql(query, engine)
    return df
