class ExecutorError(Exception):
    def __init__(self, code: str, message: str, status: int = 409):
        super().__init__(message)
        self.code = code
        self.message = message
        self.status = status


def invalid(message: str = "请求参数无效") -> ExecutorError:
    return ExecutorError("invalid_request", message, 422)
