# Azure OpenAI's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-azureopenai-setting-api-key = An Azure OpenAI resource key: in the Azure portal open the Azure OpenAI resource, then Resource Management > Keys and Endpoint, and copy KEY 1.
usage-azureopenai-setting-endpoint = The resource endpoint from the same Keys and Endpoint page, e.g. https://my-resource.openai.azure.com. Only HTTPS is accepted.
usage-azureopenai-setting-deployment = The deployment name to check, as listed under Deployments in Azure AI Foundry (https://ai.azure.com), e.g. chat-prod.
usage-azureopenai-setting-api-version = Optional. "v1" for the OpenAI-compatible v1 API, or a dated API version. Defaults to 2024-10-21.

## The report: that the deployment answers, and what it runs. Resource,
## deployment, model, and API version names stay as written.

# The plan line; $deployment is the deployment's name, such as chat-prod.
usage-azureopenai-deployment-plan = Deployment: { $deployment }
usage-azureopenai-reachable = Deployment reachable
usage-azureopenai-resource = Resource
usage-azureopenai-deployment = Deployment
usage-azureopenai-model = Model
usage-azureopenai-api-version = API version
