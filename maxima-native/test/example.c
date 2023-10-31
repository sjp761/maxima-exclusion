#include <stdio.h>
#include <unistd.h>
#include <stdint.h>
#include <stdlib.h>
#include <Windows.h>

typedef const char* (*maxima_get_last_error_t)();
typedef size_t (*maxima_init_logger_t)();

// Concurrency Functions
typedef size_t (*maxima_create_runtime_t)(void** runtime_out);

// Service Functions
typedef size_t (*maxima_is_service_valid_t)(uint8_t* valid_out);
typedef size_t (*maxima_is_service_running_t)(uint8_t* running_out);
typedef size_t (*maxima_register_service_t)();
typedef size_t (*maxima_start_service_t)(void** runtime);
typedef uint8_t (*maxima_check_registry_validity_t)();
typedef size_t (*maxima_request_registry_setup_t)(void** runtime);

// Authentication Functions
typedef size_t (*maxima_login_t)(void** runtime, const char** token_out);

// Maxima-Object Functions
typedef void* (*maxima_mx_create_t)();
typedef size_t (*maxima_mx_set_access_token_t)(void** runtime, void** mx, const char* token);
typedef size_t (*maxima_mx_start_lsx_t)(void** runtime, void** mx);
typedef size_t (*maxima_mx_consume_lsx_events_t)(void** runtime, void** mx, char*** events_out, unsigned int* event_count_out);
typedef size_t (*maxima_mx_free_lsx_events_t)(char** events, unsigned int event_count);
typedef size_t (*maxima_find_owned_offer_t)(void** runtime, void** mx, const char* game_slug, const char** offer_id_out);
typedef size_t (*maxima_get_local_display_name_t)(void** runtime, void** mx, const char** display_name_out);

// Game Functions
typedef size_t (*maxima_launch_game_t)(void** runtime, void** mx, const char* offer_id);

#define DefineProc(mod, type) type##_t type = (type##_t) GetProcAddress(mod, #type);

#define ERR_CHECK_LE 2
#define ValidateRet(func) \
    { \
        size_t code = func; \
        if (code != 0) \
        { \
            if (code == ERR_CHECK_LE) \
            { \
                const char* errStr = maxima_get_last_error(); \
                printf("Function '%s' failed: %s\n", #func, errStr); \
            } \
            else \
            { \
                printf("Function '%s' failed: %d\n", #func, (int)code); \
            } \
            return 0; \
        } \
    }

int main() {
	HMODULE mod = LoadLibrary("maxima.dll");

    DefineProc(mod, maxima_get_last_error);
    DefineProc(mod, maxima_init_logger);

    // Concurrency Functions
    DefineProc(mod, maxima_create_runtime);

    // Service Functions
    DefineProc(mod, maxima_is_service_valid);
    DefineProc(mod, maxima_is_service_running);
    DefineProc(mod, maxima_register_service);
    DefineProc(mod, maxima_start_service);
    DefineProc(mod, maxima_check_registry_validity);
    DefineProc(mod, maxima_request_registry_setup);

    // Maxima Object Functions
    DefineProc(mod, maxima_mx_create);
    DefineProc(mod, maxima_mx_set_access_token);
    DefineProc(mod, maxima_mx_start_lsx);
    DefineProc(mod, maxima_mx_consume_lsx_events);
    DefineProc(mod, maxima_mx_free_lsx_events);
    DefineProc(mod, maxima_find_owned_offer);
    DefineProc(mod, maxima_get_local_display_name);

    // Authentication Functions
    DefineProc(mod, maxima_login);

    // Game Functions
    DefineProc(mod, maxima_launch_game);

    //_putenv_s("MAXIMA_LOG_LEVEL", "debug");
    ValidateRet(maxima_init_logger());

    void* runtime;
    ValidateRet(maxima_create_runtime(&runtime));

    printf("Validating service...\n");

    uint8_t serviceValid;
    ValidateRet(maxima_is_service_valid(&serviceValid));

    if (!serviceValid) {
        printf("Registering service...\n");
        ValidateRet(maxima_register_service());
        sleep(1);
    }

    printf("Ensuring service is running...\n");

    uint8_t serviceRunning;
    ValidateRet(maxima_is_service_running(&serviceRunning));

    if (!serviceRunning) {
        printf("Starting service...\n");
        ValidateRet(maxima_start_service(&runtime));
    }

    if (!maxima_check_registry_validity())
    {
        printf("Requesting registry setup\n");
        ValidateRet(maxima_request_registry_setup(&runtime));
    }

    const char* token = NULL;
    ValidateRet(maxima_login(&runtime, &token));

    void* maxima = maxima_mx_create();
    ValidateRet(maxima_mx_set_access_token(&runtime, &maxima, token));

    const char* username = NULL;
    ValidateRet(maxima_get_local_display_name(&runtime, &maxima, &username));
    printf("Welcome %s!\n", username);
    
    const char* offerId = NULL;
    ValidateRet(maxima_find_owned_offer(&runtime, &maxima, "star-wars-battlefront-2", &offerId));

    printf("Starting LSX server...\n");
    ValidateRet(maxima_mx_start_lsx(&runtime, &maxima));

    printf("Launching game (%s)...\n", offerId);
    ValidateRet(maxima_launch_game(&runtime, &maxima, offerId));

    while (1) {
        char** events;
        unsigned int event_count;
        ValidateRet(maxima_mx_consume_lsx_events(&runtime, &maxima, &events, &event_count));

        for (int i = 0; i < event_count; i++)
        {
            const char* event = events[i];
            printf("LSX Event: %s\n", event);
        }

        maxima_mx_free_lsx_events(events, event_count);
        sleep(0.05);
    }

    printf("Done");
    return 0;
}