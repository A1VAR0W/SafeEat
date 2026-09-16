export class ApiError extends Error {
  constructor(
    public statusCode: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}
export const notFound = () => new ApiError(404, 'NOT_FOUND', 'No hemos encontrado este elemento.');
export const unauthorized = () =>
  new ApiError(401, 'UNAUTHORIZED', 'Tu sesión no es válida. Inicia sesión de nuevo.');
