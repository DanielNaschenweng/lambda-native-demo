package br.com.oeratech.ingress.exception;

public class BrokerException extends RuntimeException {
    public BrokerException(String message, Throwable cause) {
        super(message, cause);
    }
}
