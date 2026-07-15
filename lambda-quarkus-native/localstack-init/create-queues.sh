#!/bin/bash
# Executado pelo LocalStack quando o serviço fica pronto (ready.d)
awslocal sqs create-queue --queue-name orders-queue
awslocal sqs create-queue --queue-name webhooks-queue
echo "SQS queues created: orders-queue, webhooks-queue"
