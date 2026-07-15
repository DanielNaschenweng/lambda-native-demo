package br.com.oeratech.ingress;

import br.com.oeratech.ingress.exception.BrokerException;
import io.quarkus.runtime.StartupEvent;
import jakarta.enterprise.context.ApplicationScoped;
import jakarta.enterprise.event.Observes;
import jakarta.inject.Inject;
import org.jboss.logging.Logger;
import software.amazon.awssdk.services.sqs.SqsClient;
import software.amazon.awssdk.services.sqs.model.MessageAttributeValue;

import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

@ApplicationScoped
public class SqsPublisher {

    private static final Logger LOG = Logger.getLogger(SqsPublisher.class);

    @Inject
    SqsClient sqs;

    @Inject
    RouteConfigService routeConfigService;

    /** Cache de nome da fila -> URL, para não chamar GetQueueUrl a cada request. */
    private final Map<String, String> queueUrlCache = new ConcurrentHashMap<>();

    /**
     * Pré-resolve as URLs das filas durante o cold start do Lambda.
     *
     * Sem isso, a primeira invocação de cada rota paga o custo de uma
     * chamada GetQueueUrl antes do SendMessage. Com o pré-aquecimento
     * esse custo é pago na inicialização e amortizado por todas as
     * invocações quentes subsequentes da mesma instância.
     */
    void onStart(@Observes StartupEvent ev) {
        routeConfigService.queueNames().forEach(queueName -> {
            try {
                resolveQueueUrl(queueName);
                LOG.infof("queue URL pre-warmed: %s", queueName);
            } catch (Exception e) {
                // Falha não-fatal: a URL será resolvida novamente na primeira request.
                LOG.warnf("queue pre-warm failed for %s (will retry on first request): %s",
                        queueName, e.getMessage());
            }
        });
    }

    public String publish(String queueName, String body, String traceId,
                          String correlationId, String routeKey) {
        try {
            String queueUrl = resolveQueueUrl(queueName);
            Map<String, MessageAttributeValue> attributes =
                    buildAttributes(traceId, correlationId, routeKey);

            String messageId = sqs.sendMessage(req -> req
                    .queueUrl(queueUrl)
                    .messageBody(body)
                    .messageAttributes(attributes))
                    .messageId();

            LOG.infof("message sent to SQS: queue=%s messageId=%s", queueName, messageId);
            return messageId;
        } catch (Exception e) {
            // Invalida o cache: a fila pode ter sido recriada com outra URL.
            queueUrlCache.remove(queueName);
            throw new BrokerException("falha ao publicar no SQS: " + e.getMessage(), e);
        }
    }

    private String resolveQueueUrl(String queueName) {
        return queueUrlCache.computeIfAbsent(queueName,
                name -> sqs.getQueueUrl(req -> req.queueName(name)).queueUrl());
    }

    private Map<String, MessageAttributeValue> buildAttributes(String traceId,
                                                               String correlationId,
                                                               String routeKey) {
        Map<String, MessageAttributeValue> attributes = new HashMap<>();
        attributes.put("route-key", stringAttribute(routeKey));
        if (traceId != null) {
            attributes.put("trace-id", stringAttribute(traceId));
        }
        if (correlationId != null) {
            attributes.put("correlation-id", stringAttribute(correlationId));
        }
        return attributes;
    }

    private MessageAttributeValue stringAttribute(String value) {
        return MessageAttributeValue.builder()
                .dataType("String")
                .stringValue(value)
                .build();
    }
}
