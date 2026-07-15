package br.com.oeratech.ingress;

import com.amazonaws.services.lambda.runtime.events.APIGatewayV2HTTPResponse;

import java.util.Map;

final class HttpResponses {

    private HttpResponses() {
    }

    static APIGatewayV2HTTPResponse error(int status, String message) {
        String body = "{\"error\":\"" + message.replace("\"", "\\\"") + "\"}";
        return APIGatewayV2HTTPResponse.builder()
                .withStatusCode(status)
                .withBody(body)
                .withHeaders(Map.of("content-type", "application/json"))
                .build();
    }

    static APIGatewayV2HTTPResponse accepted(String messageId) {
        String body = "{\"message_id\":\"" + messageId.replace("\"", "\\\"") + "\"}";
        return APIGatewayV2HTTPResponse.builder()
                .withStatusCode(202)
                .withBody(body)
                .withHeaders(Map.of("content-type", "application/json"))
                .build();
    }
}
