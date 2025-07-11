#include <Arduino.h>
#include <Wire.h>
#include <SensirionI2CSgp41.h>
#include <SensirionGasIndexAlgorithm.h>
#include <Adafruit_NeoPixel.h>

#define LED_PIN    8
#define NUM_LEDS   1

// Correct constructor: specify algorithm type and sampling interval
SensirionGasIndexAlgorithm vocAlgo(GasIndexAlgorithm_ALGORITHM_TYPE_VOC, 1.0f);
SensirionGasIndexAlgorithm noxAlgo(GasIndexAlgorithm_ALGORITHM_TYPE_NOX, 1.0f);
SensirionI2CSgp41 sgp41;

Adafruit_NeoPixel rgbLed(NUM_LEDS, LED_PIN, NEO_GRB + NEO_KHZ800);


// Time in seconds needed for NOx conditioning
uint16_t conditioning_s = 10;

void setup() {
    Serial.begin(115200);
    rgbLed.begin();
    rgbLed.show(); // Initialize all pixels to “off”.
    while (!Serial) {
        delay(100);
    }

    // Use GPIO 4 = SDA, GPIO 5 = SCL
    Wire.begin(4, 5);

    uint16_t error;
    char errorMessage[256];

    sgp41.begin(Wire);

    uint8_t serialNumberSize = 3;
    uint16_t serialNumber[serialNumberSize];

    error = sgp41.getSerialNumber(serialNumber);

    if (error) {
        Serial.print("Error trying to execute getSerialNumber(): ");
        errorToString(error, errorMessage, 256);
        Serial.println(errorMessage);
    } else {
        Serial.print("SerialNumber: 0x");
        for (size_t i = 0; i < serialNumberSize; i++) {
            uint16_t value = serialNumber[i];
            Serial.print(value < 4096 ? "0" : "");
            Serial.print(value < 256 ? "0" : "");
            Serial.print(value < 16 ? "0" : "");
            Serial.print(value, HEX);
        }
        Serial.println();
    }

    uint16_t testResult;
    error = sgp41.executeSelfTest(testResult);
    if (error) {
        Serial.print("Error trying to execute executeSelfTest(): ");
        errorToString(error, errorMessage, 256);
        Serial.println(errorMessage);
    } else if (testResult != 0xD400) {
        Serial.print("executeSelfTest failed with error: ");
        Serial.println(testResult);
    }
}

void loop() {
    
    uint16_t error;
    char errorMessage[256];
    uint16_t defaultRh = 0x8000;  // Default relative humidity
    uint16_t defaultT = 0x6666;   // Default temperature
    uint16_t srawVoc = 0;
    uint16_t srawNox = 0;

    Adafruit_NeoPixel pixel(NUM_LEDS, LED_PIN, NEO_GRB + NEO_KHZ800);
    delay(1000);

    if (conditioning_s > 0) {
        error = sgp41.executeConditioning(defaultRh, defaultT, srawVoc);
        conditioning_s--;
    } else {
        error = sgp41.measureRawSignals(defaultRh, defaultT, srawVoc, srawNox);
    }

    if (error) {
        Serial.print("Error reading signals: ");
        errorToString(error, errorMessage, 256);
        Serial.println(errorMessage);
        return;
    }

    int32_t voc_index = vocAlgo.process(srawVoc);
    int32_t nox_index = noxAlgo.process(srawNox);

    Serial.print("SRAW_VOC: ");
    Serial.print(srawVoc);
    Serial.print("\tSRAW_NOx: ");
    Serial.println(srawNox);

    Serial.print("VOC Index: ");
    Serial.print(voc_index);
    Serial.print("\tNOx Index: ");
    Serial.println(nox_index);

    rgbLed.setBrightness(255);
    if (nox_index > 30) {
        rgbLed.setPixelColor(0, rgbLed.Color(255, 0, 255)); // magenta
    } else if (voc_index > 155) {
        rgbLed.setPixelColor(0, rgbLed.Color(255, 0, 0));  // red
    } else if (voc_index > 125) {
        rgbLed.setPixelColor(0, rgbLed.Color(255, 105, 180));
    } else if (voc_index > 90) { // 80 i
        rgbLed.setPixelColor(0, rgbLed.Color(255, 255, 0));  // yellow   
        rgbLed.setBrightness(64);
    } else {
        // rgbLed.setPixelColor(0, rgbLed.Color(0, 255, 0));  // green
        rgbLed.setPixelColor(0, rgbLed.Color(21, 27, 28));  // royal conciertto
        rgbLed.setBrightness(255);

        // rgbLed.setPixelColor(0, rgbLed.Color(54, 69, 73));  // Royal Concerto teal
        // rgbLed.setBrightness(64);  // 25% brightness
    }

    rgbLed.show();
}