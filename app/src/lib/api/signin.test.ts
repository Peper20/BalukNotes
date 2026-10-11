import { afterEach, expect, it, vi } from "vitest";
import { isSignIn, onSignInRequired, requireSignIn, resetSignIn, signInMessage, signInRequired } from "./signin";

afterEach(resetSignIn);

it("the first 401 shows the sign-in screen once", () => {
  const shown = vi.fn();
  onSignInRequired(shown);
  expect(signInRequired()).toBe(false);
  requireSignIn();
  requireSignIn();
  expect(signInRequired()).toBe(true);
  expect(shown).toHaveBeenCalledTimes(1);
});

it("a listener that comes late still hears that sign-in is needed", () => {
  requireSignIn();
  const shown = vi.fn();
  onSignInRequired(shown);
  expect(shown).toHaveBeenCalledTimes(1);
});

it("only a 401 is a sign-in error", () => {
  expect(isSignIn({ status: 401 })).toBe(true);
  for (const e of [{ status: 0 }, { status: 403 }, { status: 404 }, new Error("x"), null, undefined, "401"]) {
    expect(isSignIn(e), String(e)).toBe(false);
  }
});

it("the form text for a failed sign-in", () => {
  expect(signInMessage({ status: 401, message: "wrong login or password" })).toBe("Неверный логин или пароль");
  expect(signInMessage({ status: 429, retryAfter: 12 })).toBe("Слишком много попыток, подождите 12 с");
  expect(signInMessage({ status: 429, retryAfter: null })).toBe("Слишком много попыток, подождите немного");
  expect(signInMessage({ status: 0 })).toBe("Сервер не отвечает");
  expect(signInMessage({ status: 500, message: "internal error" })).toBe("internal error");
  expect(signInMessage(null)).toBe("Не удалось войти");
});
