package br.com.oeratech.ingress;

import br.com.oeratech.ingress.dto.RouteTarget;
import br.com.oeratech.ingress.exception.BadRequestException;
import br.com.oeratech.ingress.exception.NotFoundException;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.networknt.schema.ValidationMessage;
import jakarta.enterprise.context.ApplicationScoped;
import jakarta.inject.Inject;
import org.jboss.logging.Logger;

import java.util.Set;
import java.util.stream.Collectors;

@ApplicationScoped
public class IngressService {

    private static final Logger LOG = Logger.getLogger(IngressService.class);
    private static final ObjectMapper MAPPER = new ObjectMapper();

    @Inject
    RouteConfigService routeConfigService;

    @Inject
    SqsPublisher sqsPublisher;

    public String process(String routeKey, String body, String traceId, String correlationId) {

        RouteTarget target = routeConfigService.findRoute(routeKey)
                .orElseThrow(() -> {
                    LOG.warnf("route not found: %s", routeKey);
                    return new NotFoundException("rota não encontrada: " + routeKey);
                });

        if (target.schema() != null) {
            validateBody(body, target, routeKey);
        }

        String messageId = sqsPublisher.publish(target.queueName(), body, traceId, correlationId, routeKey);

        LOG.infof("message published: queue=%s routeKey=%s", target.queueName(), routeKey);
        return messageId;
    }

    private void validateBody(String body, RouteTarget target, String routeKey) {
        JsonNode instance;
        try {
            instance = MAPPER.readTree(body);
        } catch (Exception e) {
            LOG.warnf("body is not valid JSON for route %s", routeKey);
            throw new BadRequestException("corpo da requisição não é JSON válido");
        }

        Set<ValidationMessage> errors = target.schema().validate(instance);
        if (!errors.isEmpty()) {
            String messages = errors.stream()
                    .map(ValidationMessage::getMessage)
                    .collect(Collectors.joining(", "));
            LOG.warnf("schema validation failed for route %s: %s", routeKey, messages);
            throw new BadRequestException(messages);
        }
    }
}
