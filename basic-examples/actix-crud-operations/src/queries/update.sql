UPDATE users
SET name = COALESCE($1, name),
    age = COALESCE($3, age)
WHERE email = $2;
