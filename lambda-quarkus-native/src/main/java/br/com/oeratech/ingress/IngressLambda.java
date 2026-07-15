package br.com.oeratech.ingress;

import br.com.oeratech.ingress.exception.BadRequestException;
import br.com.oeratech.ingress.exception.BrokerException;
import br.com.oeratech.ingress.exception.NotFoundException;
import com.amazonaws.services.lambda.runtime.Context;
import com.amazonaws.services.lambda.runtime.LambdaLogger;
import com.amazonaws.services.lambda.runtime.RequestHandler;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2HTTPEvent;
import com.amazonaws.services.lambda.runtime.events.APIGatewayV2HTTPResponse;
import jakarta.inject.Inject;
import jakarta.inject.Named;

import java.util.Map;

/**
 * Gateway de ingestão: recebe HTTP via API Gateway, valida o contrato
 * e enfileira no SQS. O roteamento é decidido pelo header X-Route-Key.
 */
@Named("ingressLambda")
public class IngressLambda implements RequestHandler<APIGatewayV2HTTPEvent, APIGatewayV2HTTPResponse> {

    @Inject
    IngressService ingressService;

    @Override
    public APIGatewayV2HTTPResponse handleRequest(APIGatewayV2HTTPEvent event, Context context) {
        LambdaLogger logger = context.getLogger();

        // API Gateway HTTP API v2 normaliza headers para minúsculo
        Map<String, String> headers = event.getHeaders();
        String routeKey = headers != null ? headers.get("x-route-key") : null;

        if (routeKey == null || routeKey.isBlank()) {
            logger.log("WARN: missing X-Route-Key header");
            return HttpResponses.error(400, "X-Route-Key ausente");
        }

        logger.log("INFO: processing route-key=" + routeKey);

        String traceId = headers.get("x-trace-id");
        String correlationId = headers.get("x-correlation-id");
        String body = event.getBody() != null ? event.getBody() : "";

        try {
            String messageId = ingressService.process(routeKey, body, traceId, correlationId);
            return HttpResponses.accepted(messageId);
        } catch (BadRequestException e) {
            logger.log("WARN: bad request: " + e.getMessage());
            return HttpResponses.error(400, e.getMessage());
        } catch (NotFoundException e) {
            logger.log("WARN: not found: " + e.getMessage());
            return HttpResponses.error(404, e.getMessage());
        } catch (BrokerException e) {
            logger.log("ERROR: broker error: " + e.getMessage());
            return HttpResponses.error(500, "erro ao publicar no SQS");
        }
    }
}
