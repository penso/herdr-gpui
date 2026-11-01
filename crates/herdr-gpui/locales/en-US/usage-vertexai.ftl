# Vertex AI's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-vertexai-setting-project = The Google Cloud project id whose Vertex AI quotas to read, when gcloud has none set (`gcloud config set project PROJECT_ID`). Sign in with `gcloud auth application-default login`; the account needs Cloud Monitoring read access (roles/monitoring.viewer) on the project.

## The report: the highest share of any Vertex AI quota used in the last
## day, and the Google Cloud project. Project IDs stay as gcloud names
## them.

usage-vertexai-peak-quota = Peak quota
usage-vertexai-google-cloud = Google Cloud
usage-vertexai-project = Project
# No project is configured; the command stays as written.
usage-vertexai-project-not-set = Not set: run `gcloud config set project PROJECT_ID`
