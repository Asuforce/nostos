import sys
import tomllib


def check(config):
    if str(config.get("vars", {}).get("HTTP_JOBS_ENABLED", "")).lower() == "true":
        if config.get("triggers", {}).get("crons", []):
            raise ValueError("HTTP notifications require empty Cron configuration")


if __name__ == "__main__":
    with open(sys.argv[1], "rb") as source:
        check(tomllib.load(source))
