// Глава 1: классы эквивалентности и граничные значения.
// Сборка и тест: g++ -std=c++20 -Wall -Wextra цена.cpp && ./a.out  → OK
#include <bits/stdc++.h>
#include <cassert>
using namespace std;

// Требование: 0–6 лет — 0 ₽, 7–17 — 500 ₽, 18–64 — 1000 ₽, 65–120 — 700 ₽,
// иной возраст — исключение invalid_argument.
int price_ok(int age) {
    if (age < 0 || age > 120) throw invalid_argument("age");
    if (age <= 6) return 0;
    if (age <= 17) return 500;
    if (age <= 64) return 1000;
    return 700;
}

// region: bug
int price(int age) {
    if (age < 0 || age > 120) throw invalid_argument("age");
    if (age <= 6)  return 0;
    if (age < 17)  return 500;    // по требованию: age <= 17
    if (age <= 64) return 1000;
    return 700;
}
// endregion: bug

template <class F> string run(F f, int age) {
    try { return to_string(f(age)); } catch (const invalid_argument&) { return "err"; }
}

int main() {
    // по одному представителю класса — ошибка не видна
    for (int age : {-5, 3, 12, 40, 90, 200}) assert(run(price, age) == run(price_ok, age));
    // граничные значения — ловят ровно 17
    vector<int> diff;
    for (int age : {-1, 0, 6, 7, 17, 18, 64, 65, 120, 121})
        if (run(price, age) != run(price_ok, age)) diff.push_back(age);
    assert(diff == vector<int>{17});
    assert(run(price, 17) == "1000" && run(price_ok, 17) == "500");
    // полная проверка эталона против требования
    for (int age = -1000; age <= 1000; ++age) {
        string want = age < 0 || age > 120 ? "err" : age <= 6 ? "0" : age <= 17 ? "500" : age <= 64 ? "1000" : "700";
        assert(run(price_ok, age) == want);
    }
    puts("OK");
}
