from datetime import datetime, timedelta
from rq import Queue
from rq.job import Job
from utils.redis import redis_client
from utils.store import store
import httpx
import json
import os
from dotenv import load_dotenv
load_dotenv()

API_URL = os.getenv("API_URL")

q = Queue(connection=redis_client)

def check_in_task(clerk_id: str, check_in_id: str):
    """Triggered Celery task for handling a scheduled check-in."""
    # Logic to handle the check-in when triggered
    # For example, updating the status in the database, sending a notification, etc.
    check_ins = store.get((clerk_id,), "check_ins")

    if not check_ins:
        return
    else:
        result = [x for x in check_ins.value["entries"] if x.get("id") == check_in_id]
        if not len(result):
            return
        else:
            check_in = result[0]
            check_ins.value["entries"] = [x for x in check_ins.value["entries"] if x.get("id") != check_in_id]
            store.put((clerk_id,), "check_ins", check_ins.value)

            payload = {
                "clerk_id": clerk_id,
                "check_in_id": check_in_id,
                "timestamp": check_in["specified_time"],
                "tz_offset": check_in["tz_offset"],
                "reason": check_in["reason"],
                "is_reminder": check_in["is_reminder"]
            }
            headers = {
                "x-secret-key": os.getenv("SECRET_KEY")
            }

            try:
                response = httpx.post(API_URL + "/run-check-in", json=payload, headers=headers, timeout=10)
                response.raise_for_status()
                print(f"Check-in processed: {response.json()}")
            except Exception as e:
                print(f"Failed to process check-in (A): {e}")

            print(f"Triggered check-in for clerk_id: {clerk_id}, check_in_id: {check_in_id}")


def schedule_task(eta: str, clerk_id: str, check_in_id: str):
    """Schedule a Celery task to run at a specified time.

    Args:
        eta: UTC timestamp in ISO format.
        clerk_id: Clerk ID.
        check_in_id: Store check-in entry ID.
    """
    execution_time = datetime.fromisoformat(eta)
    q.enqueue_at(execution_time, check_in_task, clerk_id, check_in_id, job_id=check_in_id)
    # q.enqueue_at()
    # q.enqueue_in(timedelta(minutes=delay_minutes), check_in_task, clerk_id, check_in_id, job_id=check_in_id)

    print(f"Scheduled task.")


def cancel_task(check_in_id: str):
    job = Job.fetch(check_in_id, connection=redis_client)
    job.cancel()
    print(f"Cancelled task.")