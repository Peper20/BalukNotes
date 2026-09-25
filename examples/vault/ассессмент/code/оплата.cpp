// Глава 1: идемпотентность через ключ запроса.
// Сборка и тест: g++ -std=c++20 -Wall -Wextra оплата.cpp && ./a.out  → OK
#include <bits/stdc++.h>
#include <cassert>
using namespace std;

// region: pay
struct Result { bool ok; long long balance; };

class Wallet {
    long long balance_ = 1000;
    unordered_map<string, Result> done_;       // ключ запроса → ответ
public:
    Result pay(const string& key, long long amount) {
        if (auto it = done_.find(key); it != done_.end())
            return it->second;          // повтор: без списания
        Result r{amount <= balance_, balance_};
        if (r.ok) r.balance = balance_ -= amount;
        done_[key] = r;
        return r;
    }
};
// endregion: pay

int main() {
    Wallet w;
    auto a = w.pay("k1", 100);
    auto b = w.pay("k1", 100);                 // повтор
    assert(a.ok && b.ok && a.balance == 900 && b.balance == 900);
    auto c = w.pay("k2", 100);                 // новый платёж
    assert(c.ok && c.balance == 800);
    auto d = w.pay("k3", 5000);                // отказ тоже запоминается
    auto e = w.pay("k3", 5000);
    assert(!d.ok && !e.ok && d.balance == 800 && e.balance == 800);
    puts("OK");
}
