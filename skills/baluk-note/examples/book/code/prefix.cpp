// Working code of the book: compiled and tested; a chapter takes a region.
#include <cassert>
#include <vector>

std::vector<long long> prefix(const std::vector<int>& a) {
    std::vector<long long> p(a.size() + 1, 0);
    for (size_t i = 0; i < a.size(); i++) p[i + 1] = p[i] + a[i];
    return p;
}

// region: query
long long query(const std::vector<long long>& p, int l, int r) {
    return p[r + 1] - p[l];
}
// endregion: query

int main() {
    assert(query(prefix({3, 1, 4, 1, 5}), 1, 3) == 6);
}
